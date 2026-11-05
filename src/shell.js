// 应用外壳：欢迎页、标题栏（logo / 菜单 / 项目面板）、文件菜单、打开项目。
//
// 图本身仍然由 app.js 负责；这里通过全局 bootProject() 换项目。

(function shell() {
  const api = window.__TAURI__ ? window.__TAURI__.core : null;
  // 浏览器预览：没有 Tauri，窗口控制与原生对话框都不存在，数据是预先导出的静态 json
  const desktop = !!api;
  const RECENT_KEY = "clew.recentProjects";
  const VERSION = "0.1.0";

  // ── 样式 ────────────────────────────────────────────────────────
  const style = document.createElement("style");
  style.textContent = `
    /* 只有第一行是项目色渐变，第二行工具栏保持原色 */
    #top{--tint-a:rgba(79,134,201,.34);--tint-b:rgba(79,134,201,.09);
      background:#0f1216!important;
      height:78px!important;flex-direction:column!important;align-items:stretch!important;
      gap:0!important;padding:0!important;overflow:visible!important;
      border-bottom:none!important}

    /* 标题栏控件：hover 叠一层半透明白，不换成实色
       菜单项撑满整行不留圆角，图标和项目按钮保持胶囊 */
    .tb-icon,.tb-menu-item,.tb-project{
      background:transparent;
      border:1px solid transparent;
      transition:background .12s ease,border-color .12s ease;
    }
    .tb-icon:hover,.tb-menu-item:hover,.tb-project:hover{
      background:rgba(255,255,255,.15);
      border-color:rgba(255,255,255,.10);
    }
    .tb-menu-item.open,.tb-project.open{
      background:rgba(255,255,255,.19);
      border-color:rgba(255,255,255,.12);
    }
    #graph{inset:78px 0 0 0!important}
    #focus{inset:78px 0 0 0!important}
    #tb-row1{display:flex;align-items:center;gap:8px;height:44px;flex:none;
      padding-left:10px;background:
        linear-gradient(96deg, var(--tint-a) 0%, var(--tint-b) 58%,
          rgba(26,28,32,.92) 100%),
        rgba(26,28,32,.92)}
    #tb-row2{display:flex;align-items:center;gap:8px;height:34px;flex:none;
      padding:0 12px 0 10px;background:#0f1216;
      border-top:1px solid rgba(255,255,255,.05)}
    #tb-winctl{display:flex;align-items:stretch;margin-left:auto;flex:none}
    .win-btn{width:46px;height:44px;background:none;border:none;color:#9aa0a8;
      font:inherit;font-size:12px;cursor:pointer;line-height:1;
      display:grid;place-items:center}
    /* 页面里有一条给画布连线用的 svg{position:absolute;inset:0} 全局规则，
       图标里的 svg 必须显式回到行内盒，否则会飞到视口左上角 */
    .win-btn svg{position:static;inset:auto;display:block;width:12px;height:12px;
      pointer-events:none}
    .win-btn:hover{background:rgba(255,255,255,.12);color:#e8eaed}
    .win-btn.close:hover{background:#c42b1c;color:#fff}
    .tb-brand{color:var(--acc);font-weight:600;white-space:nowrap;letter-spacing:.5px;
      display:flex;align-items:center;gap:6px;flex:none}
    .tb-brand img{width:20px;height:20px;flex:none;display:block;pointer-events:none}
    .w-brand .tb-brand img{width:28px;height:28px}
    .tb-icon{color:var(--tx);cursor:pointer;padding:6px 12px;border-radius:4px;font:inherit;font-size:17px;
      line-height:1;display:flex;align-items:center;flex:none}
    .tb-project{display:flex;align-items:center;gap:8px;color:#dfe1e5;font:inherit;
      padding:3px 10px 3px 4px;border-radius:6px;cursor:pointer;max-width:280px}
    .pj-avatar{width:20px;height:20px;border-radius:5px;display:grid;place-items:center;
      font-size:9px;font-weight:600;color:#fff;flex:none}
    .pj-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .pj-caret{color:#9aa0a8;flex:none;display:flex;align-items:center}
    .pj-caret svg{position:static;inset:auto;display:block;width:12px;height:12px}

    /* 项目面板：仿 PyCharm 的宽下拉 */
    #project-pop{position:fixed;z-index:80;width:460px;max-height:70vh;overflow:auto;
      background:#2b2d30;border:1px solid #393b40;border-radius:8px;padding:4px;
      box-shadow:0 16px 40px #000a;scrollbar-width:thin}
    #project-pop[hidden]{display:none}
    .pp-item{display:flex;align-items:center;gap:10px;width:100%;background:none;
      border:none;color:#dfe1e5;font:inherit;text-align:left;padding:7px 10px;
      border-radius:5px;cursor:pointer}
    .pp-item:hover{background:#2e436e}
    .pp-item .ico{width:18px;text-align:center;color:#9aa0a8;flex:none}
    .pp-item .ico svg{position:static;inset:auto;display:block;width:14px;height:14px;margin:0 auto}
    .pp-sep{height:1px;background:#393b40;margin:4px 6px}
    .pp-group{color:#6f737a;font-size:11px;padding:8px 10px 4px;letter-spacing:.3px}
    .pp-row{display:flex;align-items:center;gap:10px;padding:7px 10px;border-radius:5px;
      cursor:pointer}
    .pp-row:hover{background:#2e436e}
    .pp-avatar{width:26px;height:26px;border-radius:6px;display:grid;place-items:center;
      font-size:10px;font-weight:600;color:#fff;flex:none}
    .pp-meta{min-width:0}
    .pp-name{color:#dfe1e5;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .pp-path{color:#6f737a;font-size:11px;overflow:hidden;text-overflow:ellipsis;
      white-space:nowrap;margin-top:1px}
    .pp-hint{color:#6f737a;font-size:11px;padding:6px 10px 10px}

    .tb-status{color:var(--dim);font-size:11px;white-space:nowrap;overflow:hidden;
      text-overflow:ellipsis;max-width:34vw;margin-left:auto}
    .tb-status.err{color:#f0883e}

    #tb-menu{display:flex;align-items:stretch;gap:0;flex:none}
    #tb-menu[hidden]{display:none}
    .tb-menu-item{color:var(--tx);cursor:pointer;padding:0 14px;height:44px;border:none;border-radius:0;
      font:14px/1 "LXGW WenKai Mono","LXGW WenKai",monospace;
      display:flex;align-items:center;flex:none}

    .tb-pop{position:fixed;top:46px;background:#161b22;border:1px solid var(--bd);
      border-radius:6px;padding:4px;z-index:96;box-shadow:0 10px 28px #0009;min-width:200px}
    .tb-pop[hidden]{display:none}
    .tb-pop button{display:block;width:100%;text-align:left;background:none;border:none;
      color:var(--tx);padding:8px 14px;border-radius:4px;cursor:pointer;
      font:14px/1.2 "LXGW WenKai Mono","LXGW WenKai",monospace;
      white-space:nowrap}
    .tb-pop button:hover{background:#22303f}
    .tb-pop .sep{height:1px;background:var(--bd);margin:4px 6px}

    #tb-help{position:fixed;inset:0;z-index:90;display:grid;place-items:center;
      background:#0b0d11cc}
    #tb-help[hidden]{display:none}
    #tb-help .card{background:#161b22;border:1px solid var(--bd);border-radius:8px;
      padding:20px 24px;max-width:520px;line-height:1.8}
    #tb-help h3{margin:0 0 8px;color:var(--acc);font-size:14px}
    #tb-help kbd{background:#232b36;border:1px solid var(--bd);border-radius:3px;
      padding:0 4px;font:inherit;font-size:11px}
    #tb-help .close{float:right;cursor:pointer;color:var(--dim)}

    /* ── 欢迎页（仿 PyCharm） ─────────────────────────────────── */
    /* 顶部让出标题栏：欢迎页期间窗口按钮必须够得着，不然开错了关不掉 */
    #welcome{position:fixed;inset:44px 0 0 0;z-index:95;display:flex;background:#1e1f22;
      font:13px/1.5 "Segoe UI",system-ui,sans-serif;color:#dfe1e5}
    #welcome[hidden]{display:none}
    body.welcome-open #top{height:44px!important}
    body.welcome-open #tb-row2{display:none!important}
    body.welcome-open #project-button{display:none!important}
    body.welcome-open #tb-burger{display:none!important}
    body.welcome-open #graph{inset:44px 0 0 0!important}
    /* 标题栏文字：进项目后是品牌蓝，欢迎页是普通白字的欢迎语 */
    .tb-brand .tb-title{color:inherit}
    body.welcome-open .tb-brand .tb-title{color:#dfe1e5;font-weight:400;letter-spacing:0}
    #w-side{width:268px;flex:none;background:#1e1f22;border-right:1px solid #2b2d30;
      padding:16px 10px;display:flex;flex-direction:column;gap:4px}
    .w-brand{display:flex;align-items:center;gap:12px;padding:4px 10px 18px}
    .w-brand .t{font-size:15px;font-weight:600;color:#dfe1e5;line-height:1.25}
    .w-brand .v{font-size:11px;color:#6f737a}
    .w-brand .auth{font-size:11px;color:#6f737a}
    .w-brand .tb-brand img{width:36px;height:36px}
    .w-nav{display:flex;flex-direction:column;gap:2px}
    .w-nav button{display:flex;align-items:center;gap:8px;background:none;border:none;
      color:#dfe1e5;font:inherit;text-align:left;padding:8px 10px;border-radius:6px;
      cursor:pointer}
    .w-nav button:hover{background:#2b2d30}
    .w-nav button.active{background:#2e436e;color:#e8edf7}
    .w-foot{margin-top:auto;color:#6f737a;font-size:11px;padding:8px 10px}

    #w-main{flex:1;min-width:0;display:flex;flex-direction:column;padding:20px 26px}
    .w-toolbar{display:flex;align-items:center;gap:10px;padding-bottom:14px;
      border-bottom:1px solid #2b2d30}
    .w-search{flex:1;display:flex;align-items:center;gap:8px;color:#6f737a}
    .w-search input{flex:1;background:none;border:none;color:#dfe1e5;font:inherit;
      outline:none;padding:4px 0}
    .w-search input::placeholder{color:#6f737a}
    .w-actions{display:flex;gap:8px}
    /* PyCharm 欢迎页右侧那排按钮：灰底、细边、圆角 6、等高 */
    .w-actions button{height:34px;padding:0 16px;background:#393b40;border:1px solid #4b4e55;
      border-radius:6px;color:#dfe1e5;font:inherit;cursor:pointer}
    .w-actions button:hover{background:#45484c;border-color:#5a5e63}
    .w-actions button:active{background:#313335}

    #w-recent{list-style:none;margin:10px 0 0;padding:0;overflow:auto;flex:1;
      scrollbar-width:thin}
    #w-note{color:#6f737a;font-size:11.5px;padding:10px 12px;margin:8px 0 0;
      border:1px solid #2b2d30;border-radius:6px;background:#232529;line-height:1.6}
    #w-note[hidden]{display:none}
    #w-note.err{color:#f0883e;border-color:#4a3326;background:#2a2118}
    #w-recent li{display:flex;align-items:center;gap:12px;padding:10px;border-radius:8px;
      cursor:pointer}
    #w-recent li:hover{background:#2b2d30}
    .w-avatar{width:34px;height:34px;flex:none;border-radius:8px;display:grid;
      place-items:center;font-size:12px;font-weight:600;color:#fff;letter-spacing:.5px}
    .w-meta{min-width:0}
    .w-name{color:#dfe1e5;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .w-path{color:#6f737a;font-size:11px;overflow:hidden;text-overflow:ellipsis;
      white-space:nowrap;margin-top:2px}
    .w-hint{color:#6f737a;padding:26px 10px;line-height:1.9}
  `;
  document.head.appendChild(style);

  // ── 共用小工具 ──────────────────────────────────────────────────
  const LOGO_IMG = '<img src="logo.png" alt="" width="20" height="20">';

  // 全软件唯一一套图标：12 见方、1.5 线宽、圆头圆角、fill none、currentColor。
  // 窗口按钮照 Windows 标题栏的标准画法，最大化是单框，还原是前后两个错开的框。
  function icon(body) {
    return (
      '<svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor"' +
      ' stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
      body +
      "</svg>"
    );
  }

  const ICON = {
    min: icon('<path d="M2 6h8"/>'),
    max: icon('<rect x="2.25" y="2.25" width="7.5" height="7.5"/>'),
    restore: icon('<path d="M4 4V2.25h5.75V8H8"/><rect x="2.25" y="4" width="5.75" height="5.75"/>'),
    close: icon('<path d="M2.5 2.5l7 7M9.5 2.5l-7 7"/>'),
    chevronDown: icon('<path d="M2.75 4.5L6 7.75l3.25-3.25"/>'),
    folder: icon('<path d="M1.75 3.25h3.1l1.15 1.5h4.25v5.5H1.75z"/>'),
  };


  const AVATAR_COLORS = ["#c94f4f", "#4f86c9", "#8f4fc9", "#c9864f", "#4fa87c", "#c94f86"];

  function normPath(p) {
    return String(p || "")
      .replace(/^\\\\\?\\/, "")   // Windows canonicalize 的 \\?\ 前缀
      .replace(/\\\\+/g, "\\");   // 早期版本存进 localStorage 的重复反斜杠
  }

  function basename(p) {
    return normPath(p).split(/[\\/]/).filter(Boolean).pop() || normPath(p);
  }

  function initials(name) {
    const parts = String(name).split(/[-_.\s]+/).filter(Boolean);
    if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
    return name.slice(0, 2).toUpperCase();
  }

  function colorFor(name) {
    let h = 0;
    for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
    return AVATAR_COLORS[h % AVATAR_COLORS.length];
  }

  function avatar(name, cls) {
    const el = document.createElement("span");
    el.className = cls;
    el.textContent = initials(name);
    el.style.background = colorFor(name);
    return el;
  }

  function hexToRgb(hex) {
    const h = hex.replace("#", "");
    return [
      parseInt(h.slice(0, 2), 16),
      parseInt(h.slice(2, 4), 16),
      parseInt(h.slice(4, 6), 16),
    ].join(",");
  }

  // 顶栏底色跟着项目走：每个项目一个色，铺成低饱和的渐变
  function applyTitleTint(path) {
    const name = path ? basename(path) : "";
    const rgb = hexToRgb(name ? colorFor(name) : "#4f86c9");
    const top = document.getElementById("top");
    top.style.setProperty("--tint-a", "rgba(" + rgb + ",.34)");
    top.style.setProperty("--tint-b", "rgba(" + rgb + ",.09)");
  }

  // ── 最近项目 ────────────────────────────────────────────────────
  function recent() {
    let list = [];
    try {
      list = JSON.parse(localStorage.getItem(RECENT_KEY) || "[]");
    } catch {
      list = [];
    }
    const seen = new Set();
    return list
      .map((p) => ({ path: normPath(p.path), name: p.name }))
      .filter((p) => {
        if (!p.path || seen.has(p.path)) return false;
        seen.add(p.path);
        return true;
      });
  }

  function remember(path) {
    const clean = normPath(path);
    if (!clean) return;
    const list = recent().filter((p) => p.path !== clean);
    list.unshift({ path: clean, name: basename(clean) });
    localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 20)));
  }

  // ── 欢迎页 ──────────────────────────────────────────────────────
  const welcome = document.createElement("div");
  welcome.id = "welcome";
  welcome.innerHTML = `
    <aside id="w-side">
      <div class="w-brand">
        <span class="tb-brand"></span>
        <span><span class="t">Clew</span><br><span class="v">${VERSION}</span>
        <br><span class="auth">qingchu4n</span></span>
      </div>
      <nav class="w-nav"><button class="active">项目</button></nav>
      <div class="w-foot">调用图入口在文件菜单里</div>
    </aside>
    <main id="w-main">
      <div class="w-toolbar">
        <label class="w-search">
          <input id="w-query" spellcheck="false" placeholder="搜索项目">
        </label>
        <div class="w-actions"><button id="w-open">打开新项目</button></div>
      </div>
      <div id="w-note" hidden></div>
      <ul id="w-recent"></ul>
    </main>`;
  document.body.appendChild(welcome);
  // 侧栏用同一张位图的放大版，标题栏是 20px，这里 36px 由 CSS 接管
  welcome.querySelector(".w-brand .tb-brand").innerHTML =
    '<img src="logo.png" alt="" width="36" height="36">';

  const recentList = welcome.querySelector("#w-recent");
  const queryInput = welcome.querySelector("#w-query");
  const noteEl = welcome.querySelector("#w-note");

  // 欢迎页自己的提示条：预览模式的说明和打开失败的原因都落在这里，不再只写进被盖住的顶栏
  function setNote(text, isError) {
    noteEl.textContent = text || "";
    noteEl.hidden = !text;
    noteEl.classList.toggle("err", !!isError);
  }

  if (!desktop) {
    setNote(
      "浏览器预览：图来自 __clew_graph.json（由 clew export 生成）。" +
        "打开项目、窗口控制、记笔记这类回后端的动作在这里不可用。",
    );
  }

  function renderRecent() {
    const q = queryInput.value.trim().toLowerCase();
    const list = recent().filter(
      (p) => !q || p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q),
    );
    recentList.innerHTML = "";
    if (!list.length) {
      const li = document.createElement("li");
      li.className = "w-hint";
      li.style.cursor = "default";
      li.textContent = q
        ? "没有匹配的项目"
        : desktop
          ? "还没有项目。点右上角「打开新项目」，选一个 Python 目录（首次打开会自动建索引）。"
          : "预览模式没有最近项目。换项目请重跑 clew export 后刷新页面。";
      recentList.appendChild(li);
      return;
    }
    for (const p of list) {
      const li = document.createElement("li");
      const meta = document.createElement("div");
      meta.className = "w-meta";
      const name = document.createElement("div");
      name.className = "w-name";
      name.textContent = p.name;
      const path = document.createElement("div");
      path.className = "w-path";
      path.textContent = p.path;
      meta.appendChild(name);
      meta.appendChild(path);
      li.appendChild(avatar(p.name, "w-avatar"));
      li.appendChild(meta);
      li.onclick = () => openPath(p.path);
      recentList.appendChild(li);
    }
  }

  queryInput.oninput = renderRecent;
  welcome.querySelector("#w-open").onclick = () => chooseFolder();

  // 欢迎页没有项目，就没有菜单可开：左上角只留 logo 和一句欢迎语
  let brandTitle = null;
  function showWelcome() {
    renderRecent();
    document.body.classList.add("welcome-open");
    if (brandTitle) {
      brandTitle.style.display = "";
      brandTitle.textContent = "欢迎访问 Clew";
    }
    welcome.hidden = false;
  }
  function hideWelcome() {
    document.body.classList.remove("welcome-open");
    if (brandTitle) brandTitle.style.display = "none";
    welcome.hidden = true;
  }

  // ── 标题栏 ──────────────────────────────────────────────────────
  const top = document.getElementById("top");
  const entriesBox = document.getElementById("entries");
  const trail = document.getElementById("trail");

  const brand = document.createElement("span");
  brand.className = "tb-brand";
  brand.innerHTML = LOGO_IMG + '<span class="tb-title">Clew</span>';
  brandTitle = brand.querySelector(".tb-title");

  const burger = document.createElement("button");
  burger.className = "tb-icon";
  burger.id = "tb-burger";
  burger.textContent = "☰";
  burger.title = "菜单";

  const projectButton = document.createElement("button");
  projectButton.className = "tb-project";
  projectButton.id = "project-button";
  projectButton.innerHTML =
    '<span class="pj-avatar"></span><span class="pj-name"></span>' +
    '<span class="pj-caret">' + ICON.chevronDown + "</span>";

  const menu = document.createElement("div");
  menu.id = "tb-menu";
  menu.hidden = true;
  for (const [key, label] of [["file", "文件"], ["help", "帮助"]]) {
    const item = document.createElement("button");
    item.className = "tb-menu-item";
    item.dataset.menu = key;
    item.textContent = label;
    menu.appendChild(item);
  }

  const status = document.createElement("span");
  status.className = "tb-status";

  const entryLabel = document.createElement("span");
  entryLabel.textContent = "入口";
  entryLabel.style.color = "var(--dim)";

  const row1 = document.createElement("div");
  row1.id = "tb-row1";
  row1.dataset.tauriDragRegion = "";
  row1.appendChild(brand);
  row1.appendChild(burger);
  row1.appendChild(projectButton);
  row1.appendChild(menu);

  // 窗口控制按钮：系统标题栏已经关掉了（浏览器预览里没有窗口可控制）
  if (desktop) {
    const winctl = document.createElement("div");
    winctl.id = "tb-winctl";
    winctl.innerHTML =
      '<button class="win-btn" data-win="min" title="最小化">' + ICON.min + "</button>" +
      '<button class="win-btn" data-win="max" title="最大化">' + ICON.max + "</button>" +
      '<button class="win-btn close" data-win="close" title="关闭">' + ICON.close + "</button>";
    row1.appendChild(winctl);

    const win = window.__TAURI__.window.getCurrentWindow();
    const maxBtn = winctl.querySelector('[data-win="max"]');

    // 最大化与还原是两个形状，跟着窗口状态换
    async function syncMaxButton() {
      try {
        const isMax = await win.isMaximized();
        maxBtn.innerHTML = isMax ? ICON.restore : ICON.max;
        maxBtn.title = isMax ? "向下还原" : "最大化";
      } catch {
        /* 窗口状态读不到就保持原样 */
      }
    }
    syncMaxButton();
    win.onResized(syncMaxButton);

    winctl.addEventListener("click", function (e) {
      const b = e.target.closest("[data-win]");
      if (!b) return;
      if (b.dataset.win === "min") win.minimize();
      else if (b.dataset.win === "max") win.toggleMaximize();
      else win.close();
    });
  }

  const row2 = document.createElement("div");
  row2.id = "tb-row2";
  row2.dataset.tauriDragRegion = "";
  row2.appendChild(entryLabel);
  row2.appendChild(entriesBox);
  const sep = document.createElement("span");
  sep.textContent = "|";
  sep.style.color = "var(--dim)";
  row2.appendChild(sep);
  row2.appendChild(trail);
  row2.appendChild(status);

  top.innerHTML = "";
  top.dataset.tauriDragRegion = "";
  top.appendChild(row1);
  top.appendChild(row2);

  // ── 项目面板 ────────────────────────────────────────────────────
  const projectPop = document.createElement("div");
  projectPop.id = "project-pop";
  projectPop.hidden = true;
  document.body.appendChild(projectPop);

  function projectRow(p, onClick) {
    const row = document.createElement("div");
    row.className = "pp-row";
    const meta = document.createElement("div");
    meta.className = "pp-meta";
    const name = document.createElement("div");
    name.className = "pp-name";
    name.textContent = p.name;
    const path = document.createElement("div");
    path.className = "pp-path";
    path.textContent = p.path;
    meta.appendChild(name);
    meta.appendChild(path);
    row.appendChild(avatar(p.name, "pp-avatar"));
    row.appendChild(meta);
    row.onclick = onClick;
    return row;
  }

  function groupLabel(text) {
    const d = document.createElement("div");
    d.className = "pp-group";
    d.textContent = text;
    return d;
  }

  function sepLine() {
    const d = document.createElement("div");
    d.className = "pp-sep";
    return d;
  }

  function renderProjectPop() {
    projectPop.innerHTML = "";

    const openItem = document.createElement("button");
    openItem.className = "pp-item hl";
    openItem.innerHTML = '<span class="ico">' + ICON.folder + "</span>打开新项目";
    openItem.onclick = () => {
      closeProjectPop();
      chooseFolder();
    };
    projectPop.appendChild(openItem);
    projectPop.appendChild(sepLine());

    const list = recent();
    const current = list.find((p) => p.path === currentPath);
    const others = list.filter((p) => p.path !== currentPath);

    projectPop.appendChild(groupLabel("打开的项目"));
    if (current) {
      projectPop.appendChild(projectRow(current, () => closeProjectPop()));
    } else {
      const hint = document.createElement("div");
      hint.className = "pp-hint";
      hint.textContent = "当前没有打开的项目";
      projectPop.appendChild(hint);
    }

    if (others.length) {
      projectPop.appendChild(sepLine());
      projectPop.appendChild(groupLabel("最近的项目"));
      for (const p of others.slice(0, 12)) {
        projectPop.appendChild(
          projectRow(p, () => {
            closeProjectPop();
            openPath(p.path);
          }),
        );
      }
    }
  }

  function openProjectPop() {
    renderProjectPop();
    const r = projectButton.getBoundingClientRect();
    projectPop.style.left = Math.round(r.left) + "px";
    projectPop.style.top = Math.round(r.bottom + 4) + "px";
    projectPop.hidden = false;
    projectButton.classList.add("open");
  }

  function closeProjectPop() {
    projectPop.hidden = true;
    projectButton.classList.remove("open");
  }

  projectButton.onclick = function (e) {
    e.stopPropagation();
    if (projectPop.hidden) openProjectPop();
    else closeProjectPop();
  };

  function renderProjectButton() {
    const nameEl = projectButton.querySelector(".pj-name");
    const avEl = projectButton.querySelector(".pj-avatar");
    if (currentPath) {
      const name = basename(currentPath);
      nameEl.textContent = name;
      avEl.textContent = initials(name);
      avEl.style.background = colorFor(name);
      projectButton.title = normPath(currentPath);
    } else {
      nameEl.textContent = "打开项目";
      avEl.textContent = "";
      avEl.style.background = "rgba(255,255,255,.18)";
      projectButton.title = "选择一个 Python 项目";
    }
    applyTitleTint(currentPath);
  }

  // ── 文件菜单 ────────────────────────────────────────────────────
  const pop = document.createElement("div");
  pop.className = "tb-pop";
  pop.hidden = true;
  document.body.appendChild(pop);

  function closePop() {
    pop.hidden = true;
    for (const b of menu.children) b.classList.remove("open");
  }

  function popAt(anchor, items) {
    pop.innerHTML = "";
    for (const item of items) {
      if (item === "-") {
        const s = document.createElement("div");
        s.className = "sep";
        pop.appendChild(s);
        continue;
      }
      const b = document.createElement("button");
      b.textContent = item.label;
      b.onclick = function () {
        closePop();
        item.run();
      };
      pop.appendChild(b);
    }
    const r = anchor.getBoundingClientRect();
    pop.style.left = Math.round(r.left) + "px";
    pop.hidden = false;  }

  // ── 菜单栏模式 ──────────────────────────────────────────────────
  let menuMode = false;
  function setMenuMode(on) {
    menuMode = on;
    menu.hidden = !on;
    burger.style.display = on ? "none" : "";
    projectButton.style.display = on ? "none" : "";
    if (!on) closePop();
  }

  burger.onclick = function (e) {
    e.stopPropagation();
    setMenuMode(!menuMode);
  };

  document.addEventListener("keydown", function (e) {
    if (e.key !== "Escape") return;
    if (!projectPop.hidden) closeProjectPop();
    else if (!pop.hidden) closePop();
    else if (menuMode) setMenuMode(false);
  });

  // 点任何地方都收起下拉；用捕获阶段，免得被标题栏元素吃掉
  window.addEventListener(
    "mousedown",
    function (e) {
      if (
        !projectPop.hidden &&
        !projectPop.contains(e.target) &&
        !projectButton.contains(e.target)
      ) {
        closeProjectPop();
      }
      if (!pop.hidden && !pop.contains(e.target)) closePop();
      if (menuMode && !row1.contains(e.target) && !pop.contains(e.target)) {
        setMenuMode(false);
      }
    },
    true,
  );

  // ── 帮助 ────────────────────────────────────────────────────────
  const help = document.createElement("div");
  help.id = "tb-help";
  help.hidden = true;
  help.innerHTML =
    '<div class="card"><span class="close">✕</span>' +
    "<h3>Clew " + VERSION + "</h3>" +
    "<p>从入口开始读调用图。点 <kbd>◆</kbd> 进入实现，类卡片按需长出方法目录。</p>" +
    "<p>空白处按住拖动画布，右下角 minimap 点哪儿跳哪儿。</p>" +
    "<p>第二行下拉换入口；文件菜单里可以打开或关闭项目。</p></div>";
  document.body.appendChild(help);
  help.querySelector(".close").onclick = function () {
    help.hidden = true;
  };
  help.onclick = function (e) {
    if (e.target === help) help.hidden = true;
  };

  menu.addEventListener("click", function (e) {
    const item = e.target.closest("[data-menu]");
    if (!item) return;
    for (const b of menu.children) b.classList.toggle("open", b === item);
    if (item.dataset.menu === "file") {
      popAt(item, [
        { label: "打开项目…", run: chooseFolder },
        { label: "关闭项目", run: closeProject },
      ]);
    } else {
      closePop();
      help.hidden = false;
    }
  });

  // ── 项目操作 ────────────────────────────────────────────────────
  let currentPath = window.__CLEW_PATH__ ? normPath(window.__CLEW_PATH__) : null;

  function setStatus(text, isError) {
    status.textContent = text || "";
    status.classList.toggle("err", !!isError);
  }

  async function openPath(path) {
    if (!desktop) {
      setNote("预览模式下不能打开别的项目。换项目请重跑 clew export 后刷新页面。", true);
      showWelcome();
      return;
    }
    setStatus("正在打开（首次会先建索引）…");
    try {
      await api.invoke("open_project", { path: path });
      const data = await api.invoke("graph");
      bootProject(data);
      currentPath = normPath(path);
      remember(path);
      renderProjectButton();
      hideWelcome();
      setStatus("");
      setNote("");
    } catch (err) {
      setStatus(String(err), true);
      setNote(String(err), true);
      if (typeof tabs !== "undefined" && tabs.length === 0) showWelcome();
    }
  }

  async function chooseFolder() {
    if (!desktop) {
      setNote("预览模式下没有原生文件对话框。换项目请重跑 clew export 后刷新页面。", true);
      return;
    }
    let dir = null;
    try {
      dir = await window.__TAURI__.dialog.open({
        directory: true,
        multiple: false,
        title: "选择要分析的 Python 项目",
      });
    } catch (err) {
      setStatus(String(err), true);
      setNote(String(err), true);
      return;
    }
    if (!dir) return;
    await openPath(dir);
  }

  function closeProject() {
    if (!desktop) {
      setNote("预览模式下只有一份图，关掉就没得看了。重跑 clew export 可以换项目。", true);
      return;
    }
    setMenuMode(false);
    bootProject({});
    currentPath = null;
    renderProjectButton();
    setStatus("");
    showWelcome();
  }

  // ── 启动 ────────────────────────────────────────────────────────
  if (currentPath) remember(currentPath);
  renderProjectButton();

  // 预览模式没有项目路径，只要图里有入口就直接进画布
  const hasGraph = typeof tabs !== "undefined" && tabs.length > 0;
  if ((currentPath || !desktop) && hasGraph) {
    hideWelcome();
  } else {
    showWelcome();
  }

  window.clewShell = {
    openPath: openPath,
    chooseFolder: chooseFolder,
    closeProject: closeProject,
    setStatus: setStatus,
    setMenuMode: setMenuMode,
    showWelcome: showWelcome,
    hideWelcome: hideWelcome,
    icons: ICON,
  };
})();








