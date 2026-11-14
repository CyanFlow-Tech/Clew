<#
  设置页的 DOM 检查：活动栏齿轮、弹窗、迁移按钮的三态、欢迎页那一份。

    .\tools\settings-check.ps1

  和 side-check.ps1 同一套路：造一份带 Tauri 桩的测试页，起静态服务，
  用 headless Edge 通过 CDP 点它。桩只回答设置页用得到的命令。
#>
param(
  [int]$Port = 8898,
  [int]$Cdp = 9443
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root "target\web"

New-Item -ItemType Directory -Force -Path $web | Out-Null
Copy-Item (Join-Path $root "src\shell.js") $web -Force
Copy-Item (Join-Path $root "src\app.js") $web -Force
Copy-Item (Join-Path $root "src\themes.js") $web -Force
Copy-Item (Join-Path $root "src\logo.png") $web -Force
Copy-Item (Join-Path $root "src\index.html") $web -Force
if (-not (Test-Path (Join-Path $web "vendor"))) {
  Copy-Item (Join-Path $root "src\vendor") $web -Recurse -Force
}
Set-Content -Path (Join-Path $web "__clew_graph.json") -Value '{"cards":{},"calls":{},"entries":[]}' -Encoding UTF8

$stub = @'
      const noopWin = {
        hide() {}, show() {}, minimize() {}, toggleMaximize() {}, maximize() {},
        unmaximize() {}, close() {}, setTitle() {}, startDragging() {},
        isMaximized() { return Promise.resolve(false); },
        onCloseRequested() { return Promise.resolve(function () {}); },
        onResized() { return Promise.resolve(function () {}); },
      };
      window.__STUB_MIGRATED__ = [];
      window.__STUB_WEBVIEW__ = [];
      window.__STUB_RESET__ = [];
      window.__TAURI__ = {
        core: {
          invoke(cmd, args) {
            if (cmd === "boot") return Promise.resolve(null);
            if (cmd === "open_project") return Promise.resolve({
              state: "ready", root: "D:/stub/project", modules: 1, symbols: 1,
              calls: 0, boot_ms: 0, entries: [{ id: "m:f", label: "m:f" }],
            });
            if (cmd === "space_state") return Promise.resolve({
              home: "D:\\stub\\home",
              default_home: "D:\\stub\\home",
              pointer: "D:\\stub\\clew.home.txt",
              is_default: true,
              webview_dir: "D:\\stub\\home\\webview",
              webview_default: "D:\\stub\\home\\webview",
              webview_custom: false,
            });
            if (cmd === "reset_webview_dir") {
              window.__STUB_RESET__.push(1);
              return Promise.resolve("D:\\stub\\home\\webview");
            }
            if (cmd === "migrate_webview_dir") {
              window.__STUB_WEBVIEW__.push(args.target);
              return Promise.resolve(args.target);
            }
            if (cmd === "migrate_space") {
              window.__STUB_MIGRATED__.push(args.target);
              return Promise.resolve({
                home: args.target, files: 3, bytes: 128, items: ["projects"],
              });
            }
            if (cmd === "roots") return Promise.resolve({ favorites: [], recent: [] });
            if (cmd === "tree" || cmd === "files") return Promise.resolve([]);
            if (cmd === "scan_status") return Promise.resolve({ state: "ready" });
            return Promise.resolve({});
          },
        },
        window: { getCurrentWindow: () => noopWin },
        dialog: { open: () => Promise.resolve("D:/stub/new-space"), message: () => Promise.resolve() },
      };
'@

$html = Get-Content (Join-Path $root "src\index.html") -Raw
$anchor = "  const api = window.__TAURI__ ? window.__TAURI__.core : null;"
if (-not $html.Contains($anchor)) { throw "index.html 里找不到注入点" }
$html = $html.Replace($anchor, $stub + "`r`n" + $anchor)
Set-Content -Path (Join-Path $web "settings-harness.html") -Value $html -Encoding UTF8

$server = Start-Process -FilePath "python" -ArgumentList @("-m", "http.server", "$Port", "--directory", $web) -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
$profile = Join-Path $root "target\edge-profile-settings"
Remove-Item -Recurse -Force $profile -ErrorAction SilentlyContinue
$edge = Start-Process -FilePath "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" -ArgumentList @(
    "--headless=new", "--remote-debugging-port=$Cdp", "--user-data-dir=$profile",
    "--no-first-run", "--disable-extensions", "--disable-sync", "--disable-gpu",
    "--window-size=1440,920", "http://127.0.0.1:$Port/settings-harness.html"
) -PassThru
Start-Sleep -Seconds 10

$env:CLEW_CDP_PORT = "$Cdp"
$check = @'
(async () => {
  const errs = [];
  window.addEventListener("error", (e) => errs.push(String(e.message)));
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const q = (sel) => document.querySelector(sel);
  const out = {};
  if (!window.clewShell) return { fatal: "shell.js 没挂上" };

  // 欢迎页那一份：导航点「设置」进去，项目列表让位，里面两块目录
  const navSpace = q('.w-nav [data-w-pane="space"]');
  const navProjects = q('.w-nav [data-w-pane="projects"]');
  if (!navSpace) return { fatal: "欢迎页没有设置入口" };
  out.welcomeNav = { label: navSpace.textContent };
  navSpace.click();
  await wait(150);
  out.welcomeSpace = {
    spaceShown: !q("#w-space").hidden,
    projectsHidden: q("#w-projects").hidden,
    navActive: navSpace.classList.contains("active"),
    blocks: q("#w-space").querySelectorAll(".st-pane").length,
    headings: Array.from(q("#w-space").querySelectorAll(".st-h")).map((h) => h.textContent),
  };
  navProjects.click();
  await wait(120);
  out.welcomeBack = { spaceHidden: q("#w-space").hidden, projectsShown: !q("#w-projects").hidden };

  // 进项目，活动栏才露出来
  await window.clewShell.openPath("D:/stub/project");
  await wait(400);
  const rail = q("#side-rail");
  const gear = q("#rail-settings");
  if (!gear) return { fatal: "活动栏没有齿轮" };
  const gearBox = gear.getBoundingClientRect();
  const railBox = rail.getBoundingClientRect();
  out.gear = {
    last: rail.lastElementChild === gear,
    pinnedBottom: Math.round(railBox.bottom - gearBox.bottom) <= 8,
    aboveTabs: gearBox.top > q('#side-rail [data-tab="tree"]').getBoundingClientRect().top,
  };

  // 点齿轮只开弹窗，不该把侧栏的 tab 切走
  gear.click();
  await wait(300);
  out.open = {
    modalShown: !q("#settings").hidden,
    treeStillOn: q('#side-rail [data-tab="tree"]').classList.contains("on"),
    treePaneVisible: !q("#sp-pane-tree").hidden,
    searchPaneHidden: q("#sp-pane-search").hidden,
  };

  const modal = q("#settings");
  const navItems = modal.querySelectorAll(".st-nav-item");
  const blocks = modal.querySelectorAll(".st-pane");
  const dataPane = modal.querySelector('.st-pane[data-block="data"]');
  const cachePane = modal.querySelector('.st-pane[data-block="cache"]');
  const pathEl = dataPane.querySelector('[data-k="path"]');
  const migrateBtn = dataPane.querySelector('[data-k="migrate"]');
  const box = getComputedStyle(pathEl);
  out.initial = {
    // 两块目录放在同一个导航项下，用标题分开
    navCount: navItems.length,
    navLabels: Array.from(navItems).map((b) => b.textContent),
    blocks: blocks.length,
    headings: Array.from(modal.querySelectorAll(".st-h")).map((h) => h.textContent),
    dataButtons: Array.from(dataPane.querySelectorAll(".st-btn")).map((b) => b.textContent),
    cacheButtons: Array.from(cachePane.querySelectorAll(".st-btn")).map((b) => b.textContent),
    path: pathEl.value,
    migrateDisabled: migrateBtn.disabled,
    // 框要看得出来是框：至少一条下边框
    bottomBorder: box.borderBottomWidth + " " + box.borderBottomStyle,
  };

  // 界面缓存那块：点框只选目录，「默认」只填默认值，两者都不搬；按迁移才搬
  const cachePath = cachePane.querySelector('[data-k="path"]');
  const cacheMigrate = cachePane.querySelector('[data-k="migrate"]');
  const cacheDefault = cachePane.querySelector('[data-k="default"]');
  const cacheNote = cachePane.querySelector('[data-k="note"]');
  out.cache = {
    path: cachePath.value,
    buttons: Array.from(cachePane.querySelectorAll(".st-btn")).map((b) => b.textContent),
    // 当前就在默认位置，按钮该是灰的
    defaultDisabled: cacheDefault.disabled,
    migrateDisabled: cacheMigrate.disabled,
  };

  cachePath.click();
  await wait(300);
  out.cachePicked = {
    target: cachePath.value,
    migrateEnabled: !cacheMigrate.disabled,
  };

  cacheMigrate.click();
  await wait(500);
  out.cacheAfterMigrate = {
    calls: window.__STUB_WEBVIEW__.length,
    path: cachePath.value,
    migrateDisabled: cacheMigrate.disabled,
    defaultEnabled: !cacheDefault.disabled,
    note: cacheNote.textContent,
  };

  // 「默认」只填值：框换成默认路径、迁移按钮亮起，但一次后端调用都不该发生
  const callsBeforeDefault = window.__STUB_WEBVIEW__.length;
  cacheDefault.click();
  await wait(300);
  out.cacheDefaultFilled = {
    path: cachePath.value,
    migrateEnabled: !cacheMigrate.disabled,
    noCallYet: window.__STUB_WEBVIEW__.length === callsBeforeDefault,
  };

  // 再按迁移才真的搬回默认
  cacheMigrate.click();
  await wait(500);
  out.cacheBackToDefault = {
    calls: window.__STUB_WEBVIEW__.length,
    lastTarget: window.__STUB_WEBVIEW__[window.__STUB_WEBVIEW__.length - 1],
    migrateDisabled: cacheMigrate.disabled,
    note: cacheNote.textContent,
  };

  // 数据目录那块：点框 = 弹资源管理器，选完按钮才可点
  let opened = 0;
  const originalOpen = window.__TAURI__.dialog.open;
  window.__TAURI__.dialog.open = function () {
    opened++;
    return originalOpen.apply(this, arguments);
  };
  pathEl.click();
  await wait(250);
  window.__TAURI__.dialog.open = originalOpen;
  out.picked = {
    pickerOpened: opened,
    path: pathEl.value,
    migrateEnabled: !migrateBtn.disabled,
  };

  migrateBtn.click();
  await wait(600);
  out.afterMigrate = {
    calls: window.__STUB_MIGRATED__.length,
    path: pathEl.value,
    migrateDisabled: migrateBtn.disabled,
    // 迁移不该把人踢回欢迎页
    stillInProject: q("#welcome").hidden,
  };

  // Esc 关弹窗
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  await wait(150);
  out.esc = { modalHidden: q("#settings").hidden };
  out.errs = errs;
  return out;
})()
'@

$checkPath = Join-Path $root "target\settings-check.js"
Set-Content -Path $checkPath -Value $check -Encoding UTF8
node (Join-Path $root "tools\cdp.js") "@$checkPath"

Stop-Process -Id $edge.Id -Force -ErrorAction SilentlyContinue
Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -match "$Cdp" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Remove-Item -Recurse -Force $profile, $checkPath -ErrorAction SilentlyContinue
