param(
  [int]$Port = 8899,
  [int]$Cdp = 9444,
  [string]$Project = 'D:\CodeEverything\PythonWork\alpha-zero-general',
  [string]$Db = 'D:\Foo\clew\target\bench\alpha.db'
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root "target\web"

New-Item -ItemType Directory -Force -Path $web | Out-Null
Copy-Item (Join-Path $root "src\shell.js") $web -Force
Copy-Item (Join-Path $root "src\app.js") $web -Force
Copy-Item (Join-Path $root "src\themes.js") $web -Force
Copy-Item (Join-Path $root "src\logo.png") $web -Force
Copy-Item (Join-Path $root "src\vendor") $web -Recurse -Force

# 真实数据：index 里的整棵树，由 clew tree --json 导出给桩用
& (Join-Path $root "target\debug\clew.exe") tree $Project --db $Db --json |
  Out-File -FilePath (Join-Path $web "__tree.json") -Encoding utf8

# 造一份测试页：shell.js 加载之前塞一个 Tauri 桩。侧栏走的是 desktop 分支
# （api.invoke("tree") / ("roots")），浏览器预览那条路根本不执行。
$stub = @'
      const noopWin = {
        hide() {}, show() {}, minimize() {}, toggleMaximize() {}, maximize() {},
        unmaximize() {}, close() {}, setTitle() {}, startDragging() {},
        isMaximized() { return Promise.resolve(false); },
        onCloseRequested() { return Promise.resolve(function () {}); },
        onResized() { return Promise.resolve(function () {}); },
      };
      window.__TAURI__ = {
        core: {
          invoke(cmd, args) {
            if (cmd === "boot") return Promise.resolve({
              state: "ready", root: "D:/stub/project", modules: 88, symbols: 545,
              calls: 0, boot_ms: 0, entries: window.__STUB_ENTRIES__,
            });
            if (cmd === "tree") return window.__STUB_TREE__.then((d) => d);
            if (cmd === "files") return window.__STUB_TREE__.then((d) => d.map((f) => ({
              module: f.module, path: f.path, is_third_party: f.is_third_party,
              has_main_guard: f.has_main_guard, symbols: f.symbols.length,
            })));
            if (cmd === "symbols_in") return window.__STUB_TREE__.then(
              (d) => (d.filter((f) => f.module === args.module)[0] || { symbols: [] }).symbols
            );
            if (cmd === "entry_symbols") return window.__STUB_TREE__.then((d) => {
              const out = [];
              d.forEach((f) => f.symbols.forEach((s) => { if (s.is_entry) out.push(s); }));
              return out;
            });
            // roots 第一次调用先挂起，由检查脚本显式放行：真实窗口里它往往
            // 晚于第一次渲染，rootData 没有默认值时 release 就是这样炸的。
            // 用挂起/放行而不是 setTimeout，时序是确定的，不靠猜多久够。
            if (cmd === "roots") {
              const a = (args && args.action) || "get";
              if (a === "toggle") {
                const id = args.cardId;
                const i = window.__STUB_FAVS__.indexOf(id);
                if (i >= 0) window.__STUB_FAVS__.splice(i, 1);
                else window.__STUB_FAVS__.push(id);
              }
              if (!window.__STUB_ROOTS_READY__) {
                return new Promise((resolve) =>
                  window.__STUB_ROOTS_WAITERS__.push(() =>
                    resolve({
                      favorites: window.__STUB_FAVS__.slice(),
                      recent: window.__STUB_RECENT__.slice(),
                    })
                  )
                );
              }
              return Promise.resolve({
                favorites: window.__STUB_FAVS__.slice(),
                recent: window.__STUB_RECENT__.slice(),
              });
            }
            if (cmd === "cards") {
              const cards = {}, refs = {};
              (args.ids || []).forEach((id) => {
                const i = id.indexOf(":");
                const module = id.slice(0, i), qualname = id.slice(i + 1);
                cards[id] = {
                  id: id, module: module, qualname: qualname, title: qualname,
                  params: [], kind: "function",
                  lines: [{ k: "stmt", text: "pass", indent: 0, ln: 1 }],
                };
                refs[id] = { module: module, qualname: qualname };
              });
              return Promise.resolve({ cards: cards, calls: {}, fanin: {}, refs: refs });
            }
            return Promise.resolve({});
          },
        },
        window: { getCurrentWindow: () => noopWin },
        dialog: { open: () => Promise.resolve(null), message: () => Promise.resolve() },
      };
      window.__STUB_TREE__ = fetch("__tree.json").then((r) => r.json());
      window.__STUB_FAVS__ = [];
      window.__STUB_RECENT__ = [];
      window.__STUB_ROOTS_READY__ = false;
      window.__STUB_ROOTS_WAITERS__ = [];
      window.__STUB_ENTRIES__ = [{ id: "reference.Arena:Arena", label: "reference.Arena:Arena" }];
'@

$html = Get-Content (Join-Path $root "src\index.html") -Raw
# 桩必须在 index.html 里那段启动脚本之前，否则 boot() 走的是预览分支，
# 没有项目路径，侧栏什么都不会去取
$anchor = "  const api = window.__TAURI__ ? window.__TAURI__.core : null;"
if (-not $html.Contains($anchor)) { throw "index.html 里找不到注入点" }
$html = $html.Replace($anchor, $stub + "`r`n" + $anchor)
Set-Content -Path (Join-Path $web "harness.html") -Value $html -Encoding UTF8

$server = Start-Process -FilePath "python" -ArgumentList @("-m", "http.server", "$Port", "--directory", $web) -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
$profile = Join-Path $root "target\edge-profile-side"
Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue
$edge = Start-Process -FilePath "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" -ArgumentList @(
    "--headless=new", "--remote-debugging-port=$Cdp", "--user-data-dir=$profile",
    "--no-first-run", "--disable-extensions", "--disable-sync", "--disable-gpu",
    "--window-size=1440,920", "http://127.0.0.1:$Port/harness.html"
) -PassThru
Start-Sleep -Seconds 14

$env:CLEW_CDP_PORT = "$Cdp"
$check = @'
(async () => {
  const errs = [];
  window.addEventListener("error", (e) => errs.push(String(e.message)));
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const S = window.__clewSide;
  if (!S) return { fatal: "侧栏没有挂上，shell.js 里可能有异常" };
  const list = () => document.getElementById("sp-list");
  const rows = () => Array.from(document.querySelectorAll("#sp-list .sp-row"));
  const files = () => rows().filter((el) => el.dataset.mod);
  const dirs = () => rows().filter((el) => el.dataset.dir);
  const syms = () => rows().filter((el) => el.dataset.card);
  const dbl = () => document.querySelectorAll("#sp-list .edot").length;
  const settle = async () => {
    let last = -1;
    for (let i = 0; i < 120; i++) {
      const n = rows().length;
      if (n === last) return n;
      last = n;
      await wait(60);
    }
    return rows().length;
  };
  // 悬窗只画视口需要的那批，统计前先滚到底把剩下的补出来
  const drawAll = async () => {
    for (let i = 0; i < 80; i++) {
      const before = rows().length;
      list().scrollTop = list().scrollHeight;
      await wait(40);
      if (rows().length === before && before >= S.rows().length) return;
    }
  };
  const openKids = (el) => {
    const n = el.nextElementSibling;
    return !!(n && Number(n.dataset.depth) > Number(el.dataset.depth));
  };
  // 逐层展开，直到没有还收着的节点；caret 决定点哪一下
  const expandAll = async (pick, caret) => {
    for (let round = 0; round < 30; round++) {
      await drawAll();
      const pending = rows().filter((el) => pick(el) && !openKids(el));
      if (!pending.length) return;
      pending.forEach((el) => (caret ? el.querySelector('[data-act="caret"]') : el).click());
      await settle();
    }
  };
  const count = () => {
    const all = S.rows();
    return {
      rows: all.length,
      dirs: all.filter((r) => r.kind === "dir").length,
      files: all.filter((r) => r.kind === "file").length,
      syms: all.filter((r) => r.kind === "sym").length,
    };
  };

  S.toggle(true);
  await settle();

  const out = { panelVisible: !document.getElementById("side-panel").hidden };
  // 这一步之前 roots 还没回来：树已经画出来了，说明不依赖它
  out.treeRenderedBeforeRoots =
    rows().length > 0 && window.__STUB_ROOTS_WAITERS__.length > 0;
  // 现在放行，后续收藏夹 / 最近使用才拿得到数据
  window.__STUB_ROOTS_READY__ = true;
  window.__STUB_ROOTS_WAITERS__.splice(0).forEach((fn) => fn());
  await wait(200);
  out.layout = {
    panelTop: Math.round(document.getElementById("side-panel").getBoundingClientRect().top),
    panelLeft: Math.round(document.getElementById("side-panel").getBoundingClientRect().left),
    listTop: Math.round(list().getBoundingClientRect().top),
    headerBottom: Math.round(document.querySelector(".sp-head").getBoundingClientRect().bottom),
    row1Bottom: Math.round(document.getElementById("tb-row1").getBoundingClientRect().bottom),
    row2Left: Math.round(document.getElementById("tb-row2").getBoundingClientRect().left),
    row2Top: Math.round(document.getElementById("tb-row2").getBoundingClientRect().top),
    panelRight: Math.round(document.getElementById("side-panel").getBoundingClientRect().right),
    viewBtnInPanel: document.getElementById("side-panel").contains(document.getElementById("sp-view-btn")),
    viewBtnInToolbar: document.getElementById("tb-row2").contains(document.getElementById("sp-view-btn")),
    graphLeft: getComputedStyle(document.getElementById("graph")).left,
    graphTop: getComputedStyle(document.getElementById("graph")).top,
  };

  out.allInitial = Object.assign({ drawn: rows().length }, count());

  await expandAll((el) => el.dataset.dir, false);
  out.afterDirs = Object.assign({ drawn: rows().length }, count());
  await expandAll((el) => el.dataset.mod, false);
  out.afterFiles = Object.assign({ drawn: rows().length }, count());
  const topLevel = count().syms;
  await expandAll((el) => el.dataset.card && el.querySelector('[data-act="caret"]'), true);
  out.allExpanded = Object.assign({ drawn: rows().length, topLevel: topLevel }, count());
  out.allExpanded.uniqueSyms = new Set(
    rows().map((el) => el.dataset.card).filter(Boolean)
  ).size;

  // 入口视图：默认展开（不用点任何东西）
  S.setView("entry");
  await settle();
  await drawAll();
  out.entry = Object.assign({ drawn: rows().length }, count());
  out.entry.entryRows = document.querySelectorAll("#sp-list .sp-row .nm.fn").length;
  out.entry.noteRows = document.querySelectorAll("#sp-list .sp-row.note").length;
  out.entry.sample = rows().slice(0, 4).map((el) => el.textContent);

  // 折叠状态独立：在入口视图里收起第一个目录，回全部函数不该受影响
  const firstDir = dirs()[0];
  if (firstDir) firstDir.click();
  await settle();
  out.entryAfterCollapse = count();

  S.setView("all");
  await settle();
  await drawAll();
  out.backToAll = count();

  S.setView("entry");
  await settle();
  out.entryStateKept = count();

  // 收藏夹
  S.setView("all");
  await settle();
  const flags = Array.from(document.querySelectorAll("#sp-list .sp-row .flag")).slice(0, 2);
  flags.forEach((el) => el.click());
  await wait(300);
  S.setView("fav");
  await settle();
  out.fav = {
    rows: rows().length,
    mods: Array.from(document.querySelectorAll("#sp-list .mod")).map((e) => e.textContent),
  };

  // 换根 + 收起
  S.setView("all");
  await settle();
  const target = document.querySelector('#sp-list .sp-row[data-card]');
  if (target) target.click();
  await wait(400);
  out.root = window.getActiveRoot ? window.getActiveRoot() : null;
  out.selected = document.querySelectorAll("#sp-list .sp-row.sel").length;

  // 收藏状态同步：卡片那侧点收藏，树上的星要立刻变，不能靠重画整棵树
  S.setView("all");
  await wait(300);
  const favRow = Array.from(document.querySelectorAll("#sp-list .sp-row")).find(
    (el) => el.dataset.card && el.querySelector("[data-fav]")
  );
  if (favRow) {
    const id = favRow.dataset.card;
    const before = favRow.querySelector("[data-fav]").classList.contains("on");
    await window.__clewToggleFavorite(id);
    await wait(300);
    const after = document.querySelector('#sp-list [data-fav="' + id + '"]');
    out.favSync = {
      id: id,
      flipped: after ? after.classList.contains("on") !== before : null,
      listRowsUnchanged: rows().length === favRow.parentElement.childElementCount
    };
    // 收藏星是两份 antd 图标，靠 CSS 切显示：这里确认切的是图标不是填充规则
    if (after) {
      const off = after.querySelector(".star-off");
      const on = after.querySelector(".star-on");
      const shown = (el) => (el ? getComputedStyle(el).display !== "none" : null);
      out.favSync.star = {
        offExists: !!off,
        onExists: !!on,
        leading: off ? off.querySelector("svg").getAttribute("viewBox") : null,
        filling: on ? on.querySelector("svg").getAttribute("viewBox") : null,
        onClass: after.classList.contains("on"),
        // 收藏状态与显示的那颗星必须一致：on 时只显示实心那份
        consistent:
          (shown(on) === true) === after.classList.contains("on") &&
          shown(off) !== shown(on),
      };
    }
    await window.__clewToggleFavorite(id);
    await wait(200);
  }

  // 卡片头也换了图标：星是两份 antd 星，关闭是 antd 的叉。
  // 画布那条全局 svg 规则会拽走没顶回来的图标，这里量一下位置与尺寸。
  const cardIcon = (sel) => {
    const node = document.querySelector(sel);
    if (!node) return null;
    const box = getComputedStyle(node);
    return {
      position: box.position,
      width: Math.round(node.getBoundingClientRect().width),
      viewBox: node.getAttribute("viewBox"),
    };
  };
  out.cardIcons = {
    starOff: cardIcon("#graph .head .btn.pin .star-off svg"),
    starOn: cardIcon("#graph .head .btn.pin .star-on svg"),
    helpClose: cardIcon("#tb-help .close svg"),
  };
  // 行尾锚点标记也是 antd 图标，尺寸跟着字号走
  const anchorIcon = document.querySelector("#graph .anchor svg");
  out.anchorIcons = {
    count: document.querySelectorAll("#graph .anchor").length,
    viewBox: anchorIcon ? anchorIcon.getAttribute("viewBox") : null,
    width: anchorIcon ? Math.round(anchorIcon.getBoundingClientRect().width) : null,
  };

  S.toggle(false);
  await wait(300);  out.closed = {
    hidden: document.getElementById("side-panel").hidden,
    graphLeft: getComputedStyle(document.getElementById("graph")).left,
  };
  out.errs = errs;
  return out;
})()
'@


$checkPath = Join-Path $root "target\side-check.js"
Set-Content -Path $checkPath -Value $check -Encoding UTF8
node (Join-Path $root "tools\cdp.js") "@$checkPath"

Stop-Process -Id $edge.Id -Force -ErrorAction SilentlyContinue
Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -match "$Cdp" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Remove-Item -Recurse -Force $profile, $checkPath -ErrorAction SilentlyContinue
