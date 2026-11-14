// 应用外壳：欢迎页、标题栏（logo / 菜单 / 项目面板）、文件菜单、打开项目。
//
// 图本身仍然由 app.js 负责；这里通过全局 bootProject() 换项目。

(function shell() {
  const api = window.__TAURI__ ? window.__TAURI__.core : null;
  // 浏览器预览：没有 Tauri，窗口控制与原生对话框都不存在，数据是预先导出的静态 json
  const desktop = !!api;
  const RECENT_KEY = "clew.recentProjects";
  const VERSION = "0.1.0";

  // 窗口版没有控制台，JS 异常和取数失败都写进应用数据目录的 clew.log，
  // 否则一次「点了没反应」事后只剩重启这一条线索
  function logToFile(text) {
    if (!desktop) return;
    api.invoke("log_line", { text: String(text) }).catch(function () {});
  }
  window.__clewLog = logToFile;
  window.addEventListener("error", function (e) {
    logToFile("error " + (e.message || "?") + " @ " + (e.filename || "") + ":" + (e.lineno || 0));
  });
  window.addEventListener("unhandledrejection", function (e) {
    const r = e.reason;
    logToFile("reject " + (r && (r.message || String(r))));
  });

  // ── 开发期热重载 ────────────────────────────────────────────────
  // dev 构建的窗口连的是 tools/dev.ps1 起的本地服务，页面源就是 127.0.0.1。
  // 盯着 shell.js 的 Last-Modified，文件一变就整页重载，改完存盘即见。
  // 发版时页面源是 tauri.localhost，这里不会触发。
  (function hotReload() {
    const host = location.hostname;
    if (host !== "127.0.0.1" && host !== "localhost") return;
    // 四个文件都要盯：只盯 shell.js 的话，改了 app.js 页面不会自己刷
    const FILES = ["index.html", "app.js", "shell.js", "themes.js"];
    let stamp = null;
    setInterval(function () {
      Promise.all(
        FILES.map(function (f) {
          return fetch(f, { method: "HEAD", cache: "no-store" })
            .then(function (r) {
              // 同一秒里改两次 mtime 分不出来，长度也算一票
              return (r.headers.get("last-modified") || "") + "/" +
                (r.headers.get("content-length") || "");
            })
            .catch(function () {
              return "?";
            });
        })
      )
        .then(function (parts) {
          const now = parts.join(",");
          if (stamp && now !== stamp) location.reload();
          stamp = now;
        })
        .catch(function () {
          /* 服务停了就等它回来 */
        });
    }, 900);
  })();

  document.addEventListener("keydown", function (e) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r") {
      e.preventDefault();
      location.reload();
    }
  });

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
    /* 活动栏是一直在的（折叠只是收起面板），所以画布左边至少让出 40px，
       否则折叠后卡片会钻到活动栏底下 */
    #graph{inset:78px 0 0 40px!important}
    #focus{inset:78px 0 0 40px!important}
    #tb-row2{margin-left:40px}
    #hint{left:56px}
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
    .tb-status.busy{display:flex;align-items:center;gap:8px;max-width:46vw;margin-left:auto}
    .tb-prog{width:110px;height:6px;border-radius:3px;background:#2b3038;overflow:hidden;flex:none}
    .tb-prog i{display:block;height:100%;background:var(--acc);width:0;
      transition:width .25s ease}
    .tb-prog-t{color:var(--dim);font-size:11px;white-space:nowrap;overflow:hidden;
      text-overflow:ellipsis}

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
    .tb-pop button:disabled{color:#5c6169;cursor:default}
    .tb-pop button:disabled:hover{background:none}
    .tb-pop .sep{height:1px;background:var(--bd);margin:4px 6px}
    .tb-pop .has-sub{display:flex;align-items:center;justify-content:space-between;gap:18px}
    .tb-pop .has-sub svg{position:static;inset:auto;display:block;width:11px;height:11px;
      opacity:.7}
    .tb-pop.sub-pop{z-index:97;top:auto;min-width:180px}

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
    #w-list{overflow:auto;flex:1;scrollbar-width:thin;margin-top:6px}
    .w-sec + .w-sec{margin-top:4px}
    #w-recent,#w-hidden{list-style:none;margin:0;padding:0}
    #w-hidden[hidden]{display:none}
    .w-sec-h{display:flex;align-items:center;gap:6px;color:#6f737a;font-size:11.5px;
      padding:8px 10px 6px;letter-spacing:.4px}
    .w-sec-h.toggle{background:none;border:none;width:100%;text-align:left;
      cursor:pointer;font:inherit;font-size:11.5px;color:#6f737a;border-radius:6px}
    .w-sec-h.toggle:hover{color:#dfe1e5}
    .w-sec-h .chev{display:inline-flex;transition:transform .15s ease}
    .w-sec-h.open .chev{transform:rotate(90deg)}
    .w-sec-h .cnt{color:#6f737a}
    .w-sec-h svg{position:static;inset:auto;display:block;width:11px;height:11px}
    #w-recent li,#w-hidden li{display:flex;align-items:center;gap:12px;padding:10px;
      border-radius:8px;cursor:pointer}
    #w-recent li:hover,#w-hidden li:hover{background:#2b2d30}
    #w-hidden li{opacity:.75}
    .w-acts{margin-left:auto;display:flex;align-items:center;gap:2px;flex:none}
    .w-nameline{display:flex;align-items:center;gap:6px;min-width:0}
    .w-nameline .w-name{min-width:0}
    .w-missing{color:#d9a03c;display:inline-flex;flex:none;cursor:help}
    .w-missing svg{position:static;inset:auto;display:block;width:12px;height:12px}
    #w-recent li.gone .w-name,#w-hidden li.gone .w-name{color:#9aa1aa}
    .w-act{width:26px;height:26px;border-radius:6px;border:none;background:none;
      color:#8b919a;display:grid;place-items:center;cursor:pointer;padding:0;flex:none}
    .w-act:hover{background:#33363a;color:#e6e8ea}
    .w-act.danger:hover{background:#3a2626;color:#e06c6c}
    .w-act svg{position:static;inset:auto;display:block;width:13px;height:13px}
    #w-confirm{position:fixed;inset:0;z-index:99;display:grid;place-items:center;
      background:#0b0d11cc}
    #w-confirm[hidden]{display:none}
    .cf-card{background:#161b22;border:1px solid var(--bd);border-radius:8px;padding:18px 20px;
      width:min(440px,86vw);box-shadow:0 18px 40px #0008}
    .cf-card h3{margin:0 0 8px;font-size:14px;color:#dfe1e5}
    .cf-card p{margin:0 0 16px;color:#9aa1aa;font-size:12.5px;line-height:1.7;
      word-break:break-all;white-space:pre-wrap}
    .cf-actions{display:flex;justify-content:flex-end;gap:8px}
    .cf-actions button{height:30px;padding:0 14px;border-radius:6px;border:1px solid #33383f;
      background:#232529;color:#dfe1e5;font:inherit;font-size:12.5px;cursor:pointer}
    .cf-actions button:hover{background:#2b3037}
    .cf-actions .danger{border-color:#5a3232;background:#3a2626;color:#f0a5a5}
    .cf-actions .danger:hover{background:#4a2d2d}
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
    chevronRight: icon('<path d="M4.5 2.75L7.75 6 4.5 9.25"/>'),
    folder: icon('<path d="M1.75 3.25h3.1l1.15 1.5h4.25v5.5H1.75z"/>'),
    eye: icon(
      '<path d="M1.4 6S3.1 3.2 6 3.2 10.6 6 10.6 6 8.9 8.8 6 8.8 1.4 6 1.4 6z"/>' +
        '<circle cx="6" cy="6" r="1.35"/>',
    ),
    eyeOff: icon(
      '<path d="M1.4 6S3.1 3.2 6 3.2 10.6 6 10.6 6 8.9 8.8 6 8.8 1.4 6 1.4 6z"/>' +
        '<path d="M2.3 2.3l7.4 7.4"/>',
    ),
    trash: icon(
      '<path d="M2.2 3.4h7.6"/><path d="M4.6 3.4V2.3h2.8v1.1"/>' +
        '<path d="M3.3 3.4l.5 6.1h4.4l.5-6.1"/>',
    ),
    warn: icon(
      '<path d="M6 1.9L11.1 10.2H0.9L6 1.9z"/><path d="M6 4.7v2.3"/>' +
        '<circle cx="6" cy="8.6" r="0.45"/>',
    ),
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
      .map((p) => ({ path: normPath(p.path), name: p.name, hidden: !!p.hidden }))
      .filter((p) => {
        if (!p.path || seen.has(p.path)) return false;
        seen.add(p.path);
        return true;
      });
  }

  function saveRecent(list) {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 200)));
  }

  function remember(path) {
    const clean = normPath(path);
    if (!clean) return;
    const list = recent();
    const prev = list.find((p) => p.path === clean);
    const rest = list.filter((p) => p.path !== clean);
    rest.unshift({ path: clean, name: basename(clean), hidden: prev ? prev.hidden : false });

    // 可见项目最多二十条，隐藏的不占这个额度
    let visible = 0;
    const kept = [];
    for (const p of rest) {
      if (p.hidden) {
        kept.push(p);
        continue;
      }
      if (visible >= 20) continue;
      visible++;
      kept.push(p);
    }
    saveRecent(kept);
  }

  function setHidden(path, hidden) {
    const clean = normPath(path);
    saveRecent(recent().map((p) => (p.path === clean ? { ...p, hidden } : p)));
  }

  function forget(path) {
    const clean = normPath(path);
    saveRecent(recent().filter((p) => p.path !== clean));
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
      <div id="w-list">
        <section class="w-sec">
          <div class="w-sec-h">最近项目</div>
          <ul id="w-recent"></ul>
        </section>
        <section class="w-sec" id="w-hidden-sec" hidden>
          <button class="w-sec-h toggle" id="w-hidden-toggle" type="button">
            <span class="chev">${ICON.chevronRight}</span>
            <span>隐藏项目</span>
            <span class="cnt" id="w-hidden-cnt"></span>
          </button>
          <ul id="w-hidden" hidden></ul>
        </section>
      </div>
    </main>`;
  // 先藏着：启动时可能直接打开上次的项目，不能先闪一下欢迎页
  welcome.hidden = true;
  document.body.appendChild(welcome);
  // 侧栏用同一张位图的放大版，标题栏是 20px，这里 36px 由 CSS 接管
  welcome.querySelector(".w-brand .tb-brand").innerHTML =
    '<img src="logo.png" alt="" width="36" height="36">';

  const recentList = welcome.querySelector("#w-recent");
  const hiddenSec = welcome.querySelector("#w-hidden-sec");
  const hiddenList = welcome.querySelector("#w-hidden");
  const hiddenToggle = welcome.querySelector("#w-hidden-toggle");
  const hiddenCnt = welcome.querySelector("#w-hidden-cnt");
  const queryInput = welcome.querySelector("#w-query");
  const noteEl = welcome.querySelector("#w-note");

  // 隐藏区默认折叠，展开状态记在一次会话里，不写进本地存储
  let hiddenOpen = false;
  hiddenToggle.onclick = function () {
    hiddenOpen = !hiddenOpen;
    hiddenToggle.classList.toggle("open", hiddenOpen);
    hiddenList.hidden = !hiddenOpen;
  };

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

  // 目录存在性：进欢迎页时批量查一次，删掉记录后再查一次
  let missing = {};

  async function refreshMissing() {
    if (!desktop) {
      missing = {};
      return;
    }
    const list = recent();
    if (!list.length) {
      missing = {};
      return;
    }
    try {
      const flags = await api.invoke("check_paths", { paths: list.map((p) => p.path) });
      const next = {};
      list.forEach((p, i) => {
        if (!flags[i]) next[p.path] = true;
      });
      missing = next;
    } catch (err) {
      missing = {};
    }
  }

  // 一行项目：头像、名字路径、右侧两个动作。动作区自己吃掉点击，
  // 免得点眼睛或垃圾桶的时候把项目打开。
  function welcomeRow(p) {
    const li = document.createElement("li");
    li.appendChild(avatar(p.name, "w-avatar"));

    const meta = document.createElement("div");
    meta.className = "w-meta";
    const nameline = document.createElement("div");
    nameline.className = "w-nameline";
    const name = document.createElement("div");
    name.className = "w-name";
    name.textContent = p.name;
    nameline.appendChild(name);

    // 目录被外部删掉的记录，名字后面挂一个感叹号，鼠标停上去说明原因
    const gone = !!missing[p.path];
    if (gone) {
      li.classList.add("gone");
      const badge = document.createElement("span");
      badge.className = "w-missing";
      badge.title = "路径不存在：" + p.path;
      badge.innerHTML = ICON.warn;
      nameline.appendChild(badge);
    }

    const path = document.createElement("div");
    path.className = "w-path";
    path.textContent = p.path;
    meta.appendChild(nameline);
    meta.appendChild(path);
    li.appendChild(meta);

    const acts = document.createElement("div");
    acts.className = "w-acts";
    acts.onclick = (e) => e.stopPropagation();

    const eye = document.createElement("button");
    eye.type = "button";
    eye.className = "w-act";
    eye.title = p.hidden ? "恢复显示" : "隐藏";
    eye.innerHTML = p.hidden ? ICON.eye : ICON.eyeOff;
    eye.onclick = function (e) {
      e.stopPropagation();
      setHidden(p.path, !p.hidden);
      renderRecent();
      setNote(
        p.hidden ? p.name + " 已恢复显示" : p.name + " 已隐藏，可在下方隐藏项目里恢复",
        false,
      );
    };
    acts.appendChild(eye);

    const del = document.createElement("button");
    del.type = "button";
    del.className = "w-act danger";
    del.title = "删除项目";
    del.innerHTML = ICON.trash;
    del.onclick = function (e) {
      e.stopPropagation();
      askDelete(p);
    };
    acts.appendChild(del);
    li.appendChild(acts);

    // 目录不存在时点击只给清理入口；隐藏的项目先要求恢复可见
    if (gone) {
      li.onclick = () => askDelete(p, true);
    } else if (p.hidden) {
      li.onclick = () => setNote("「" + p.name + "」已隐藏，先点眼睛图标恢复显示再打开", true);
    } else {
      li.onclick = () => openPath(p.path);
    }
    return li;
  }

  function hintRow(text) {
    const li = document.createElement("li");
    li.className = "w-hint";
    li.style.cursor = "default";
    li.textContent = text;
    return li;
  }

  function renderRecent() {
    const q = queryInput.value.trim().toLowerCase();
    const match = (p) =>
      !q || p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q);
    const all = recent();
    const visible = all.filter((p) => !p.hidden && match(p));
    const hidden = all.filter((p) => p.hidden && match(p));

    recentList.innerHTML = "";
    if (!visible.length) {
      recentList.appendChild(
        hintRow(
          q
            ? "没有匹配的项目"
            : desktop
              ? "还没有项目。点右上角「打开新项目」，选一个 Python 目录（首次打开会自动建索引）。"
              : "预览模式没有最近项目。换项目请重跑 clew export 后刷新页面。",
        ),
      );
    }
    for (const p of visible) recentList.appendChild(welcomeRow(p));

    hiddenSec.hidden = hidden.length === 0;
    hiddenCnt.textContent = hidden.length ? "（" + hidden.length + "）" : "";
    hiddenList.innerHTML = "";
    for (const p of hidden) hiddenList.appendChild(welcomeRow(p));
    if (!hidden.length) hiddenOpen = false;
    hiddenList.hidden = !hiddenOpen;
    hiddenToggle.classList.toggle("open", hiddenOpen);
  }

  // ── 删除项目确认 ────────────────────────────────────────────────
  const confirmLayer = document.createElement("div");
  confirmLayer.id = "w-confirm";
  confirmLayer.hidden = true;
  confirmLayer.innerHTML =
    '<div class="cf-card">' +
    '<h3 id="cf-title"></h3>' +
    '<p id="cf-body"></p>' +
    '<div class="cf-actions">' +
    '<button id="cf-cancel" type="button">取消</button>' +
    '<button id="cf-ok" class="danger" type="button">删除</button>' +
    "</div></div>";
  welcome.appendChild(confirmLayer);

  const cfTitle = confirmLayer.querySelector("#cf-title");
  const cfBody = confirmLayer.querySelector("#cf-body");
  const cfOk = confirmLayer.querySelector("#cf-ok");
  const cfCancel = confirmLayer.querySelector("#cf-cancel");
  let pendingDelete = null;

  function closeConfirm() {
    confirmLayer.hidden = true;
    pendingDelete = null;
  }

  function askDelete(p, missingDir) {
    pendingDelete = p;
    if (missingDir) {
      cfTitle.textContent = "目录不存在";
      cfBody.textContent =
        p.path + "\n\n这个目录已经不在了，建议把这条记录从最近项目里删掉。";
      cfOk.textContent = "删除记录";
    } else {
      cfTitle.textContent = "删除项目「" + p.name + "」";
      cfBody.textContent =
        "会删除 " + p.path + "\\.clew 索引目录，并把它从最近项目里移除。项目源码不受影响。";
      cfOk.textContent = "删除";
    }
    confirmLayer.hidden = false;
  }

  cfCancel.onclick = closeConfirm;
  confirmLayer.onclick = function (e) {
    if (e.target === confirmLayer) closeConfirm();
  };
  cfOk.onclick = async function () {
    const item = pendingDelete;
    closeConfirm();
    if (!item) return;
    try {
      // 目录已经不在时后端会直接跳过删除，历史记录在这边清掉就行
      if (desktop) await api.invoke("delete_index", { path: item.path });
    } catch (err) {
      setNote(String(err), true);
      return;
    }
    if (currentPath === item.path) closeProject();
    forget(item.path);
    renderRecent();
    if (desktop) {
      refreshMissing().then(function () {
        if (!welcome.hidden) renderRecent();
      });
    }
    setNote("「" + item.name + "」已删除，索引一并清除", false);
  };

  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && !confirmLayer.hidden) closeConfirm();
  });

  queryInput.oninput = renderRecent;
  welcome.querySelector("#w-open").onclick = () => chooseFolder();

  // 欢迎页没有项目，就没有菜单可开：左上角只留 logo 和一句欢迎语
  let brandTitle = null;
  function showWelcome() {
    renderRecent();
    // 目录可能在外面被删掉了，查完再画一遍，给缺目录的行挂上标记
    refreshMissing().then(function () {
      if (!welcome.hidden) renderRecent();
    });
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
  for (const [key, label] of [["file", "文件"], ["theme", "主题"], ["help", "帮助"]]) {
    const item = document.createElement("button");
    item.className = "tb-menu-item";
    item.dataset.menu = key;
    item.textContent = label;
    menu.appendChild(item);
  }

  const status = document.createElement("span");
  status.className = "tb-status";



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

  // 右侧子菜单：项目列表用它展开，跟主下拉分开一层
  const subPop = document.createElement("div");
  subPop.className = "tb-pop sub-pop";
  subPop.hidden = true;
  document.body.appendChild(subPop);

  function closeSub() {
    subPop.hidden = true;
    subPop.innerHTML = "";
  }

  function closePop() {
    closeSub();
    pop.hidden = true;
    for (const b of menu.children) b.classList.remove("open");
  }

  function openSub(anchor, items) {
    subPop.innerHTML = "";
    for (const item of items) {
      if (item === "-") {
        const s = document.createElement("div");
        s.className = "sep";
        subPop.appendChild(s);
        continue;
      }
      const b = document.createElement("button");
      b.textContent = item.label;
      if (item.disabled) b.disabled = true;
      if (item.title) b.title = item.title;
      b.onclick = function (e) {
        e.stopPropagation();
        if (item.disabled) return;
        closePop();
        item.run();
      };
      subPop.appendChild(b);
    }
    const r = anchor.getBoundingClientRect();
    subPop.style.left = Math.round(r.right + 4) + "px";
    subPop.style.top = Math.round(r.top) + "px";
    subPop.hidden = false;
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
      if (item.sub) {
        b.classList.add("has-sub");
        const arrow = document.createElement("span");
        arrow.innerHTML = ICON.chevronRight;
        b.appendChild(arrow);
        b.onmouseenter = function () { openSub(b, item.sub()); };
        b.onclick = function (e) {
          e.stopPropagation();
          openSub(b, item.sub());
        };
      } else {
        if (item.disabled) b.disabled = true;
        b.onclick = function () {
          closePop();
          item.run();
        };
      }
      pop.appendChild(b);
    }
    const r = anchor.getBoundingClientRect();
    pop.style.left = Math.round(r.left) + "px";
    pop.hidden = false;
  }

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
    if (!subPop.hidden) closeSub();
    else if (!projectPop.hidden) closeProjectPop();
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
      if (!pop.hidden && !pop.contains(e.target) && !subPop.contains(e.target)) {
        closePop();
      }
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
        { label: "项目列表", sub: projectListItems },
        "-",
        { label: "关闭项目", run: closeProject },
        "-",
        { label: "重建索引", run: rebuildIndex },
        { label: "删除索引", run: removeIndex },
      ]);
    } else if (item.dataset.menu === "theme") {
      popAt(item, themeItems());
    } else {
      closePop();
      help.hidden = false;
    }
  });

  // 代码配色主题：直接铺在菜单里，当前那个前面打勾
  function themeItems() {
    const list = window.CLEW_THEMES || [];
    const now = window.__clewThemeId ? window.__clewThemeId() : "";
    const items = list.map((t) => ({
      label: (t.id === now ? "✓ " : "   ") + t.name,
      run: () => {
        if (window.__clewApplyTheme) window.__clewApplyTheme(t.id);
      },
    }));
    if (!items.length) items.push({ label: "主题表没加载", disabled: true });
    return items;
  }

  // 项目列表子菜单：固定显示前几个，其余折成一行省略号，最后是管理入口
  const SUB_MENU_MAX = 8;
  function projectListItems() {
    const list = recent()
      .filter((p) => !p.hidden)
      .filter((p) => p.path !== currentPath);
    const items = list.slice(0, SUB_MENU_MAX).map((p) => ({
      label: p.name,
      run: () => openPath(p.path),
    }));
    if (list.length > SUB_MENU_MAX) {
      items.push({
        label: "…还有 " + (list.length - SUB_MENU_MAX) + " 个",
        disabled: true,
      });
    }
    if (!items.length) items.push({ label: "没有其他项目", disabled: true });
    items.push("-");
    items.push({ label: "管理项目", disabled: true, title: "稍后提供" });
    return items;
  }

  // ── 项目操作 ────────────────────────────────────────────────────
  let currentPath = window.__CLEW_PATH__ ? normPath(window.__CLEW_PATH__) : null;
  // 收藏与最近，来自 roots.json；侧栏的收藏夹 / 最近使用两个视图都读它
  let rootData = { favorites: [], recent: [] };

  function setStatus(text, isError) {
    status.classList.remove("busy");
    status.textContent = text || "";
    status.classList.toggle("err", !!isError);
  }

  // 建索引时状态栏换成进度条，估算时间做一次指数平滑，免得数字跳来跳去
  let scanTimer = null;
  let smoothEta = 0;

  function fmtEta(ms) {
    if (!ms || ms < 800) return "即将完成";
    const s = Math.round(ms / 1000);
    if (s < 60) return "还剩 " + s + " 秒";
    const m = Math.floor(s / 60);
    const rest = s % 60;
    return "还剩 " + m + " 分" + (rest ? " " + rest + " 秒" : "");
  }

  function showProgress(fraction, text) {
    const pct = Math.max(0, Math.min(100, Math.round(fraction * 100)));
    status.classList.remove("err");
    status.classList.add("busy");
    let bar = status.querySelector(".tb-prog i");
    if (!bar) {
      status.innerHTML = '<span class="tb-prog"><i></i></span><span class="tb-prog-t"></span>';
      bar = status.querySelector(".tb-prog i");
    }
    bar.style.width = pct + "%";
    status.querySelector(".tb-prog-t").textContent = text;
  }

  function stopScanWatch() {
    if (scanTimer) {
      clearInterval(scanTimer);
      scanTimer = null;
    }
    smoothEta = 0;
  }

  // 打开项目后立刻进界面，索引在后头建，这里只负责把进度画出来
  function watchScan(path) {
    stopScanWatch();
    showProgress(0, "准备建索引…");
    scanTimer = setInterval(async function () {
      let s = null;
      try {
        s = await api.invoke("scan_status");
      } catch (err) {
        return;
      }
      if (!s) return;

      if (s.state === "failed") {
        stopScanWatch();
        setStatus(s.error || "建索引失败", true);
        setNote(s.error || "建索引失败", true);
        return;
      }
      if (s.state === "ready") {
        stopScanWatch();
        setStatus("正在载入…");
        try {
          const info = await api.invoke("open_project", { path: path });
          await bootProject(info && info.entries);
          setSideOpen(true);
          setStatus("");
          setNote("");
        } catch (err) {
          setStatus(String(err), true);
        }
        return;
      }

      const eta = s.eta_ms > 0 ? s.eta_ms : 0;
      if (eta) smoothEta = smoothEta > 0 ? smoothEta * 0.7 + eta * 0.3 : eta;
      const pct = Math.round((s.fraction || 0) * 100);
      const tail = smoothEta ? " · " + fmtEta(smoothEta) : " · 估算中";
      showProgress(
        s.fraction || 0,
        (s.phase || "建索引") + " " + pct + "%" + tail
      );
    }, 400);
  }

  // 上次开着的项目：没主动关闭的话，下次启动直接打开它
  const LAST_KEY = "clew.lastProject";

  /// 读这个项目的收藏与最近使用。命令行启动不经过 enterProject、
  /// 自动重开也不经过启动块，所以两处都要能调。
  function loadRoots() {
    if (!desktop || !currentPath) return;
    api.invoke("roots", { project: currentPath, action: "get" })
      .then(function (d) {
        rootData = d || { favorites: [], recent: [] };
        favorites.sync(rootData.favorites);
        if (sideOpen) spRender();
      })
      .catch(function () {});
  }

  function enterProject(path) {
    currentPath = normPath(path);
    // 画布的工作区快照按项目分开存，app.js 用这个 key
    window.__CLEW_PROJECT__ = currentPath;
    remember(path);
    try {
      localStorage.setItem(LAST_KEY, currentPath);
    } catch (err) {
      /* 隐私模式写不进就算了 */
    }
    loadRoots();
    // 主动打开的项目不该继续留在隐藏区
    setHidden(path, false);
    renderProjectButton();
    hideWelcome();
    setNote("");
  }

  async function openPath(path) {
    if (!desktop) {
      setNote("预览模式下不能打开别的项目。换项目请重跑 clew export 后刷新页面。", true);
      showWelcome();
      return;
    }
    stopScanWatch();
    setStatus("正在打开…");
    try {
      const info = await api.invoke("open_project", { path: path });
      // 首次打开：先进项目，索引在后台建，进度画在状态栏
      if (info && info.state === "indexing") {
        enterProject(path);
        watchScan(path);
        return;
      }
      setStatus("");
      enterProject(path);
      await bootProject(info && info.entries);
      setSideOpen(true);
    } catch (err) {
      setStatus(String(err), true);
      setNote(String(err), true);
      if (typeof tabs !== "undefined" && tabs.size === 0) showWelcome();
    }
  }

  // 重建索引：删掉旧库再走一次第一次打开的流程，后台扫描照旧走进度条
  async function rebuildIndex() {
    if (!desktop || !currentPath) return;
    if (scanTimer) {
      setStatus("索引还在建立，等它完成再重建", true);
      return;
    }
    const path = currentPath;
    try {
      await api.invoke("delete_index", { path: path });
    } catch (err) {
      setStatus(String(err), true);
      return;
    }
    bootProject([]);
    setMenuMode(false);
    await openPath(path);
  }

  // 删除索引：库没了就不能留在项目里，回到欢迎页等下次重建
  async function removeIndex() {
    if (!desktop || !currentPath) return;
    if (scanTimer) {
      setStatus("索引还在建立，等它完成再删除", true);
      return;
    }
    const path = currentPath;
    const name = basename(path);
    try {
      await api.invoke("delete_index", { path: path });
    } catch (err) {
      setStatus(String(err), true);
      return;
    }
    setMenuMode(false);
    closeProject();
    setNote("「" + name + "」的索引已删除，下次打开会重新建立", false);
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
    stopScanWatch();
    setMenuMode(false);
    bootProject([]);
    currentPath = null;
    window.__CLEW_PROJECT__ = null;
    try {
      // 主动关掉的项目，下次不再自动打开
      localStorage.removeItem(LAST_KEY);
    } catch (err) {
      /* 读不到 localStorage 就算了 */
    }
    renderProjectButton();
    setStatus("");
    showWelcome();
    spSyncProject();
  }

  // ── 启动 ────────────────────────────────────────────────────────
  if (currentPath) remember(currentPath);
  renderProjectButton();

  // 启动顺序：命令行带的项目 > 上次没关掉的项目 > 欢迎页
  (async function startup() {
    const bootInfo = window.__CLEW_BOOT_INFO__;
    // 命令行启动不经过 enterProject，这里补一次，否则下次不带参数就进不去
    if (currentPath) {
      try {
        localStorage.setItem(LAST_KEY, currentPath);
      } catch (err) {
        /* 写不进就算了 */
      }
    }
    if (bootInfo && bootInfo.state === "indexing") {
      hideWelcome();
      watchScan(bootInfo.root);
      return;
    }
    // 预览模式没有项目路径，只要图里有入口就直接进画布
    const hasGraph = typeof ENTRIES !== "undefined" && ENTRIES.length > 0;
    if ((currentPath || !desktop) && hasGraph) {
      hideWelcome();
      return;
    }
    if (desktop && !currentPath) {
      let last = null;
      try {
        last = localStorage.getItem(LAST_KEY);
      } catch (err) {
        /* 读不到就当没有 */
      }
      if (last) {
        // 目录可能已经被删掉了，先问一句再决定进不进
        let alive = false;
        try {
          const rows = await api.invoke("check_paths", { paths: [last] });
          alive = !!(rows && rows[0]);
        } catch (err) {
          alive = false;
        }
        if (alive) {
          await openPath(last);
          return;
        }
        try {
          localStorage.removeItem(LAST_KEY);
        } catch (err) {
          /* 无所谓的清理 */
        }
      }
    }
    showWelcome();
  })();

  // ── 入口面板 ────────────────────────────────────────────────────
  // 收藏标记统一用五角星：未收藏描边，收藏后填色（见 .flag.on path{fill}）。
  // 这条 path 与 app.js 里的 STAR_PATH 是同一个形状，改的时候两边一起改。
  const STAR_PATH =
    "M6 1.4L7.12 4.46L10.38 4.58L7.81 6.59L8.7 9.72L6 7.9L3.3 9.72L4.19 6.59L1.63 4.58L4.88 4.46Z";
  const FLAG_SVG =
    '<svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor"' +
    ' stroke-width="1.4" stroke-linejoin="round"' +
    ' style="position:static;inset:auto;width:12px;height:12px;display:block">' +
    '<path d="' + STAR_PATH + '"/></svg>';

  function splitId(id) {
    const i = id.indexOf(":");
    return i < 0
      ? { module: id, qualname: id }
      : { module: id.slice(0, i), qualname: id.slice(i + 1) };
  }

  /// 卡片 id（module:qualname）的显示形式，模块级换成 __main__
  function showId(id) {
    const i = String(id).indexOf(":");
    if (i < 0) return String(id);
    return String(id).slice(0, i) + ":" + showQual(String(id).slice(i + 1));
  }
  /// 把命中的那一段包成 <b>，只用在排第一的结果上
  function markHit(text, needle) {
    const t = String(text || "");
    const n = String(needle || "").toLowerCase();
    if (!n) return esc(t);
    const i = t.toLowerCase().indexOf(n);
    if (i < 0) return esc(t);
    return (
      esc(t.slice(0, i)) +
      '<span class="hit">' + esc(t.slice(i, i + n.length)) + "</span>" +
      esc(t.slice(i + n.length))
    );
  }

  /// 显示名：库里的 <module> 就是 if __name__ == "__main__" 那一块
  function showQual(qual) {
    return qual === "<module>" ? "__main__" : qual;
  }

  function shortName(id) {
    const parts = splitId(id);
    // 模块级入口就是 if __name__ == "__main__" 那一块，名字叫 __main__，
    // 别再拿文件名冒充函数名
    if (parts.qualname === "<module>") return "__main__";
    return parts.qualname.split(".").pop() || parts.qualname;
  }


  // ── 收藏状态 ────────────────────────────────────────────────────
  // 单一真源 + 订阅：谁要显示收藏状态就读 favorites.has(id)，收藏变了
  // 只广播「哪个 id 变了」，各视图自己决定怎么最小更新，互不认识。
  // 早先是每个视图各自去 rootData.favorites 里 indexOf，加一个视图就得
  // 记得去补一处通知，漏掉就不同步。
  const favorites = {
    set: new Set(),
    subs: [],
    sync(list) {
      favorites.set = new Set(list || []);
    },
    has(id) {
      return !!id && favorites.set.has(id);
    },
    subscribe(fn) {
      favorites.subs.push(fn);
    },
    emit(id) {
      favorites.subs.forEach(function (fn) {
        try {
          fn(id);
        } catch (err) {
          if (window.__clewLog) window.__clewLog("favorites 订阅者出错：" + err);
        }
      });
    },
  };

  async function toggleFavorite(id) {
    if (!desktop || !currentPath) return;
    try {
      rootData = await api.invoke("roots", {
        project: currentPath,
        action: "toggle",
        cardId: id,
      });
    } catch (err) {
      return;
    }
    favorites.sync(rootData.favorites);
    favorites.emit(id);
  }

  // 卡片头上的旗标按钮走这两个口
  window.__clewIsFavorite = function (id) {
    return favorites.has(id);
  };
  window.__clewToggleFavorite = function (id) {
    return toggleFavorite(id);
  };
  // app.js 取不到卡片时用它说话，否则一次失败只剩一片空白，看不出发生了什么
  window.__clewNote = function (text, isError) {
    setNote(text, !!isError);
  };


  // app.js 在切根和打开根之后回调这里，顶栏按钮跟着变

  // 打开不算「用过」：预览时一口气点开十几个函数，全记进去就没意义了。
  // 只有用户在卡片上真的点过东西，app.js 才会喊这里（见 __clewCardUsed）。
  window.__clewRootOpened = function (rootId) {
    if (!desktop || !currentPath || !rootId) return;
    // 侧栏在文件末尾才挂上这个回调，早于此的调用直接跳过
    if (window.__clewRootChanged) window.__clewRootChanged(rootId);
  };

  window.__clewCardUsed = function (id) {
    if (!desktop || !currentPath || !id) return;
    api.invoke("roots", { project: currentPath, action: "recent", cardId: id })
      .then(function (data) {
        rootData = data;
        if (spView === "recent") spRender();
      })
      .catch(function () {});
  };


  // 启动时先把收藏读出来，卡片头上的旗标才有正确的初始状态
  loadRoots();


  // ── 侧边栏：符号树 ──────────────────────────────────────────────
  // 仿 PyCharm：最左边一条活动栏，右边一块面板。面板顶部是视图下拉，
  // 全部函数 / 入口函数 / 收藏夹。入口函数是扫描器自己认出来的，
  // 收藏夹是用户按 ★ 收起来的内容。搜索不在这里，它在独立的入口面板里。
  //
  // 渲染只画展开的那几层：文件默认收起，所以一个项目几千个定义也只有几百行
  // DOM。展开时按帧分批补，一次 append 两百行，长文件也不会卡住。
  const sideStyle = document.createElement("style");
  sideStyle.textContent = `
    /* 侧栏是标题栏下面最左的一整列：上沿接品牌行（44px）下沿，一直到底。
       面包屑那一行属于内容区，侧栏开着的时候它从侧栏右边界开始，不横穿。 */
    #side-rail{position:fixed;left:0;top:44px;bottom:0;width:40px;background:#1e1f22;
      border-right:1px solid #2b2d30;display:flex;flex-direction:column;align-items:center;
      padding-top:6px;gap:2px;z-index:42}
    .rail-btn{width:30px;height:30px;display:grid;place-items:center;background:none;
      border:none;border-radius:4px;color:#8f939b;cursor:pointer}
    .rail-btn svg{position:static;inset:auto;display:block;width:15px;height:15px}
    .rail-btn:hover{background:#2b2d30;color:#dfe1e5}
    .rail-btn.on{background:#2e436e;color:#e8edf7}

    /* 侧栏宽度可拖：所有跟着它让位的东西都读 --side-w，别各写各的数字 */
    :root{--side-w:272px}
    #side-panel{position:fixed;left:40px;top:44px;bottom:0;width:var(--side-w);background:#1e1f22;
      border-right:1px solid #2b2d30;display:flex;flex-direction:column;z-index:42;
      font:12px/1.5 "Segoe UI",system-ui,sans-serif;color:#dfe1e5}
    #side-panel[hidden]{display:none}
    body.welcome-open #side-rail,body.welcome-open #side-panel{display:none}
    body.side-open #tb-row2{margin-left:calc(40px + var(--side-w))}
    body.side-open #graph{inset:78px 0 0 calc(40px + var(--side-w))!important}
    body.side-open #focus{inset:78px 0 0 calc(40px + var(--side-w))!important}
    body.side-open #hint{left:calc(40px + var(--side-w) + 16px)}
    /* 欢迎页没有活动栏，铺满 */
    body.welcome-open #graph{inset:44px 0 0 0!important}
    body.welcome-open #focus{inset:44px 0 0 0!important}

    /* 右边那条可拖的分界线：只做命中区，不画出来，靠光标告诉你能拖 */
    #side-resize{position:absolute;top:0;right:-3px;bottom:0;width:6px;cursor:col-resize;
      z-index:6;background:none}
    body.side-resizing{cursor:col-resize}
    body.side-resizing *{user-select:none!important}

    /* 面板顶格：视图下拉就在面板里，第一行是它，下面紧接着树 */
    /* 这一栏是面板自己的头部，跟面板同色成一个整体；右边面包屑那条另算 */
    .sp-head{display:flex;align-items:center;height:34px;padding:0 4px;flex:none;
      border-bottom:1px solid #2b2d30;background:#1e1f22}
    .sp-view{display:flex;align-items:center;gap:5px;height:26px;flex:none;
      background:transparent;border:none;border-radius:4px;color:#dfe1e5;
      font:inherit;font-size:12.5px;padding:0 8px;cursor:pointer}
    .sp-view:hover,.sp-view.open{background:#2b2d30}
    .sp-view svg{position:static;inset:auto;display:block;width:11px;height:11px;
      color:#8f939b}
    .sp-list{flex:1;overflow-y:auto;overflow-x:hidden;padding:4px 0}
    /* 滚动条：细、圆、半透明，跟面板一个色系；不要系统默认那条亮灰的。
       scrollbar-width 会盖掉 ::-webkit-scrollbar 的样式，所以这里不写它。 */
    .sp-list::-webkit-scrollbar{width:10px;height:10px}
    .sp-list::-webkit-scrollbar-track{background:transparent}
    .sp-list::-webkit-scrollbar-thumb{background:#333a43;border-radius:5px;
      border:2px solid transparent;background-clip:padding-box}
    .sp-list::-webkit-scrollbar-thumb:hover{background:#4a525d;border:2px solid transparent;
      background-clip:padding-box}
    .sp-list::-webkit-scrollbar-corner{background:transparent}
    /* 搜索视图的两个输入框：并排，顶在树上方，不跟着列表滚 */
    .sp-pane{display:flex;flex-direction:column;flex:1;min-height:0}
    .sp-pane[hidden]{display:none}
    .sp-query{display:flex;flex-direction:column;gap:5px;padding:6px;
      flex:none;border-bottom:1px solid #2b2d30}
    .sp-query[hidden]{display:none}
    .sp-q-row{display:flex;align-items:center;gap:6px}
    .sp-q-label{color:#8f939b;font-size:11px;flex:none;white-space:nowrap}
    .sp-query input{flex:1 1 auto;min-width:0;background:#161a20;border:1px solid #2b2d30;
      border-radius:4px;color:var(--tx);font:inherit;font-size:12px;padding:3px 6px}
    .sp-query input::placeholder{color:#5b636e}
    .sp-query input:focus{outline:none;border-color:#2e436e}
    .sp-row.more{color:var(--acc);font-size:12px;justify-content:center}
    .sp-row.more:hover{background:#22303f}
    .sp-row .hit{color:var(--sel);font-weight:600}
    .sp-row{display:flex;align-items:center;gap:5px;height:24px;padding-right:6px;
      white-space:nowrap;cursor:pointer;color:#c9cdd3;font-size:13px}
    .sp-row:hover{background:#2b2d30}
    .sp-row.sel{background:#2e436e;color:#e8edf7}
    .sp-row.third{color:#6f737a}
    .sp-row .caret{width:13px;flex:none;display:flex;align-items:center;
      justify-content:center;color:#8f939b}
    .sp-row .caret svg{position:static;inset:auto;display:block;width:13px;height:13px}
    .sp-row .ic{width:13px;flex:none;color:#8f939b;font-size:11px;text-align:center;
      display:flex;align-items:center;justify-content:center}
    .sp-row .fname{color:#8f939b;flex:1 1 auto;min-width:0;overflow:hidden;
      text-overflow:ellipsis;white-space:nowrap}
    .sp-row .fsep{color:#4b5563;font-size:11px;flex:none}
    /* 放不下时先截文件名，函数名要保证完整可见 */
    .sp-row .nm.fn{flex:none}
    /* 入口底下那行文件名：当注释看，小一号、更暗、靠右不抢戏 */
    /* 文件名注释行和上面那行函数是一组：hover、选中、点击都算在一起 */
    .sp-row.note{height:16px;padding-bottom:6px;font-size:11.5px;color:#6f737a}
    .sp-row:hover + .sp-row.note,
    .sp-row.note:hover,
    .sp-row:has(+ .sp-row.note:hover){background:#2b2d30}
    .sp-row.sel + .sp-row.note{background:#2e436e;color:#c9cdd3}
    /* 注释行就是一条路径字符串，一种颜色；放不下就尾部省略 */
    .sp-row.note .note-file{color:#6f737a;flex:0 1 auto;min-width:0;overflow:hidden;
      text-overflow:ellipsis;white-space:nowrap}
    .sp-row .ic svg{position:static;inset:auto;display:block;width:13px;height:13px}
    .sp-row .nm{overflow:hidden;text-overflow:ellipsis}
    .sp-row .mod{color:#6f737a;font-size:11.5px;overflow:hidden;text-overflow:ellipsis;
      max-width:90px;flex:none}
    .sp-row .tail{margin-left:auto;display:flex;align-items:center;gap:6px;flex:none}
    .sp-row .edot{width:4px;height:4px;border-radius:50%;background:var(--acc);flex:none}
    .sp-row .flag{width:12px;height:12px;display:none;color:#6f737a}
    .sp-row:hover .flag,.sp-row .flag.on{display:flex;align-items:center}
    .sp-row .flag.on{color:var(--sel)}
    .sp-row .flag svg path{fill:none}
    .sp-row .flag.on svg path{fill:currentColor}
    /* 卡片头那颗星：同一形状、同一填充规则 */
    .head .btn.pin svg path{fill:none}
    .head .btn.pin.on svg path{fill:currentColor}
    .sp-row .flag:hover{color:#dfe1e5}
    .sp-empty{color:#6f737a;padding:10px 12px;font-size:12.5px;line-height:1.6}

    /* 视图下拉跟面板同色，弹出来像是面板延伸出去的一块 */
    #sp-menu{min-width:150px;background:#1e1f22;border-color:#2b2d30;
      box-shadow:0 12px 26px #000000a6}
  `;
  document.head.appendChild(sideStyle);

  const SP_VIEWS = [
    { id: "all", label: "全部函数" },
    { id: "entry", label: "入口函数" },
    { id: "fav", label: "收藏夹" },
    { id: "recent", label: "最近使用" },
  ];
  const SP_VIEW_KEY = "clew.sideView";
  let spView = "all";
  try {
    const saved = localStorage.getItem(SP_VIEW_KEY);
    if (SP_VIEWS.some((v) => v.id === saved)) spView = saved;
  } catch (err) {
    /* 读不出来就用默认 */
  }

  // 侧栏的数据按需取：先只有文件行，展开哪个文件才问哪个文件的定义。
  // 一上来就把整棵树拉过来的话，标准库那种规模是七十万个符号、上百兆 JSON，
  // 面板还没画出来就先卡住了。
  let spFiles = null; // [{module, path, is_third_party, has_main_guard, symbols}]
  let spFileByModule = new Map();
  let spEntrySyms = null; // 入口视图：全项目入口定义，通常几十条
  let spLoadedFor = null; // 这份数据属于哪个项目
  let spSeq = 0; // 渲染令牌，连点展开时只让最后一次往上画
  const spCache = new Map(); // module -> 该文件的定义
  // 三个视图各留一套展开状态：在「全部函数」里翻到的位置，切到「入口函数」
  // 再切回来还在。默认值按视图量级定，全部函数全收起，另外两个全展开。
  function spBlank() {
    return { dirs: new Set(), files: new Set(), syms: new Set() };
  }
  const spStates = {
    all: spBlank(),
    entry: spBlank(),
    fav: spBlank(),
    recent: spBlank(),
  };
  let spTree = null; // 当前视图的目录树，展开时就地取子节点

  function st() {
    return spStates[spView] || spStates.all;
  }

  // 活动栏就是侧栏的 tab：一个符号树，一个搜索，点哪个切哪个
  const rail = document.createElement("div");
  rail.id = "side-rail";
  rail.innerHTML =
    '<button type="button" class="rail-btn on" data-tab="tree" title="符号树">' +
      ICON.folder + "</button>" +
    '<button type="button" class="rail-btn" data-tab="search" title="搜索">' +
      icon('<circle cx="5.2" cy="5.2" r="3.2"/><path d="M7.6 7.6l2.8 2.8"/>') +
    "</button>";
  document.body.appendChild(rail);

  const side = document.createElement("div");
  side.id = "side-panel";
  side.hidden = true;
  side.innerHTML =
    '<div class="sp-pane" id="sp-pane-tree">' +
      '<div class="sp-head">' +
        '<button type="button" class="sp-view" id="sp-view-btn">' +
          '<span id="sp-view-label"></span>' + ICON.chevronDown +
        "</button>" +
      "</div>" +
      '<div class="sp-list" id="sp-list"></div>' +
    "</div>" +
    '<div class="sp-pane" id="sp-pane-search" hidden>' +
      '<div class="sp-query" id="sp-query">' +
        '<label class="sp-q-row"><span class="sp-q-label">路径</span>' +
          '<input id="sp-q-path" autocomplete="off" spellcheck="false"></label>' +
        '<label class="sp-q-row"><span class="sp-q-label">函数</span>' +
          '<input id="sp-q-name" autocomplete="off" spellcheck="false"></label>' +
      "</div>" +
      '<div class="sp-list" id="sp-list-search"></div>' +
    "</div>";
  document.body.appendChild(side);

  const spViewBtn = side.querySelector("#sp-view-btn");
  const spViewLabel = side.querySelector("#sp-view-label");
  const spList = side.querySelector("#sp-list");
  const spQuery = side.querySelector("#sp-query");
  const spQPath = side.querySelector("#sp-q-path");
  const spQName = side.querySelector("#sp-q-name");
  const spListSearch = side.querySelector("#sp-list-search");
  const spPaneTree = side.querySelector("#sp-pane-tree");
  const spPaneSearch = side.querySelector("#sp-pane-search");

  // ── 搜索视图 ────────────────────────────────────────────────────
  // 两个条件各自独立：路径里含这串、函数名里含这串，同时成立才留下。
  // 分页在背后是 offset/limit，前端只记已经拿到多少条。
  const SP_PAGE_SIZE = 50;
  const spSearch = { total: 0, truncated: false, hits: [], busy: false, timer: null };
  let spSearchSeq = 0;

  function spQueryText() {
    return { path: spQPath.value.trim(), name: spQName.value.trim() };
  }

  async function spSearchPage(append) {
    if (!desktop || !currentPath) return;
    if (spSearch.busy) return;
    spSearch.busy = true;
    const seq = ++spSearchSeq;
    const q = spQueryText();
    const offset = append ? spSearch.hits.length : 0;
    try {
      const page = await api.invoke("search_symbols", {
        qPath: q.path,
        qName: q.name,
        offset: offset,
        limit: SP_PAGE_SIZE,
      });
      if (seq !== spSearchSeq) return;
      spSearch.total = page.total || 0;
      spSearch.truncated = !!page.truncated;
      spSearch.hits = append ? spSearch.hits.concat(page.hits || []) : page.hits || [];
    } catch (err) {
      if (seq !== spSearchSeq) return;
      if (window.__clewLog) window.__clewLog("search_symbols 失败：" + err);
      spSearch.hits = [];
      spSearch.total = 0;
    } finally {
      spSearch.busy = false;
    }
    if (seq === spSearchSeq) spSearchRender();
  }

  /// 搜索面板自己的渲染：一页 50 条，不够就末尾挂一行「加载更多」
  function spSearchRender() {
    const q = spQueryText();
    const rows = [];
    spSearch.hits.forEach(function (h, i) {
      rows.push({
        kind: "entry",
        depth: 0,
        card: h.card_id,
        module: h.module,
        qual: h.qualname,
        file: "",
        line: 0,
        // 只有排第一的那行高亮命中片段，其余保持安静
        hot: i === 0 && !!(q.name || q.path),
        hotName: q.name,
        hotPath: q.path,
      });
      rows.push({ kind: "note", depth: 0, card: h.card_id, file: h.path });
    });
    if (spSearch.hits.length < spSearch.total) rows.push({ kind: "more", depth: 0 });

    spListSearch.innerHTML = "";
    if (!rows.length) {
      const empty = document.createElement("div");
      empty.className = "sp-empty";
      empty.textContent = spSearch.busy ? "" : "没有匹配";
      spListSearch.appendChild(empty);
      return;
    }
    const frag = document.createDocumentFragment();
    rows.forEach((r) => frag.appendChild(spRowEl(r)));
    spListSearch.appendChild(frag);
  }

  // ── 侧栏宽度可拖 ────────────────────────────────────────────────
  // 宽度只写进 --side-w，画布、面包屑行、底部提示都跟着这个变量走。
  // 拖的时候只在 rAF 里通知一次画布重排，不然每移动 1px 就重算一遍卡片。
  const SIDE_W_KEY = "clew.sideWidth";
  const SIDE_W_MIN = 200;
  const resizer = document.createElement("div");
  resizer.id = "side-resize";
  side.appendChild(resizer);

  let sideWidth = 272;
  try {
    const saved = Number(localStorage.getItem(SIDE_W_KEY));
    if (saved >= SIDE_W_MIN) sideWidth = saved;
  } catch (err) {
    /* 读不出来就用默认 */
  }

  function clampWidth(px) {
    const max = Math.max(SIDE_W_MIN + 40, Math.round(window.innerWidth * 0.6));
    return Math.max(SIDE_W_MIN, Math.min(Math.round(px), max));
  }

  let relayoutQueued = false;
  function queueRelayout() {
    if (relayoutQueued) return;
    relayoutQueued = true;
    requestAnimationFrame(function () {
      relayoutQueued = false;
      window.dispatchEvent(new Event("resize"));
    });
  }

  function applyWidth(px, save) {
    sideWidth = clampWidth(px);
    document.documentElement.style.setProperty("--side-w", sideWidth + "px");
    if (save) {
      try {
        localStorage.setItem(SIDE_W_KEY, String(sideWidth));
      } catch (err) {
        /* 隐私模式写不进就算了 */
      }
    }
    queueRelayout();
  }
  applyWidth(sideWidth, false);

  resizer.addEventListener("pointerdown", function (e) {
    e.preventDefault();
    resizer.setPointerCapture(e.pointerId);
    resizer.classList.add("dragging");
    document.body.classList.add("side-resizing");
  });
  resizer.addEventListener("pointermove", function (e) {
    if (!resizer.classList.contains("dragging")) return;
    // 面板左边界固定在活动栏右侧，宽度就是指针横坐标减掉这一段
    applyWidth(e.clientX - 40, false);
  });
  function endDrag(e) {
    if (!resizer.classList.contains("dragging")) return;
    resizer.classList.remove("dragging");
    document.body.classList.remove("side-resizing");
    try {
      resizer.releasePointerCapture(e.pointerId);
    } catch (err) {
      /* 指针已经释放了 */
    }
    applyWidth(sideWidth, true);
  }
  resizer.addEventListener("pointerup", endDrag);
  resizer.addEventListener("pointercancel", endDrag);

  const spMenu = document.createElement("div");
  spMenu.className = "tb-pop";
  spMenu.id = "sp-menu";
  spMenu.hidden = true;
  document.body.appendChild(spMenu);

  let sideOpen = false;

  function viewLabel(id) {
    const v = SP_VIEWS.filter((x) => x.id === id)[0];
    return v ? v.label : SP_VIEWS[0].label;
  }

  function syncViewLabel() {
    spViewLabel.textContent = viewLabel(spView);
  }

  function closeSpMenu() {
    spMenu.hidden = true;
    spViewBtn.classList.remove("open");
  }

  function openSpMenu() {
    spMenu.innerHTML = "";
    SP_VIEWS.forEach(function (v) {
      const b = document.createElement("button");
      b.textContent = v.label;
      b.onclick = function () {
        closeSpMenu();
        setView(v.id);
      };
      spMenu.appendChild(b);
    });
    const r = spViewBtn.getBoundingClientRect();
    spMenu.style.left = Math.round(r.left) + "px";
    spMenu.style.top = Math.round(r.bottom + 4) + "px";
    spMenu.hidden = false;
    spViewBtn.classList.add("open");
  }

  spViewBtn.addEventListener("click", function (e) {
    e.stopPropagation();
    spMenu.hidden ? openSpMenu() : closeSpMenu();
  });
  document.addEventListener("mousedown", function (e) {
    if (!spMenu.hidden && !spMenu.contains(e.target) && !spViewBtn.contains(e.target)) {
      closeSpMenu();
    }
  });

  function setView(id) {
    spView = id;
    try {
      localStorage.setItem(SP_VIEW_KEY, id);
    } catch (err) {
      /* 隐私模式写不进就算了 */
    }
    syncViewLabel();
    spRender();
  }

  // 文件节点、类节点都是就地展开，所以节点里要能查到自己的定义
  async function ensureSyms(module) {
    if (spCache.has(module)) return spCache.get(module);
    if (!desktop) return [];
    try {
      const rows = await api.invoke("symbols_in", { module: module });
      spCache.set(module, rows || []);
    } catch (err) {
      if (window.__clewLog) window.__clewLog("symbols_in " + module + " 失败：" + err);
      spCache.set(module, []);
    }
    return spCache.get(module);
  }

  async function ensureEntry() {
    if (spEntrySyms) return spEntrySyms;
    if (!desktop) return [];
    try {
      spEntrySyms = (await api.invoke("entry_symbols")) || [];
    } catch (err) {
      spEntrySyms = [];
    }
    return spEntrySyms;
  }

  // 入口视图也默认收起目录：先只看到顶层目录，点哪层看哪层。
  // （之前是切过来就把所有目录摊开，行数一多反而没法扫。）

  // 一个文件里的定义按点号嵌套：Class.method 挂在 Class 底下
  function nestSyms(syms) {
    const byQ = Object.create(null);
    syms.forEach(function (s) {
      byQ[s.qualname] = { sym: s, kids: [] };
    });
    const tops = [];
    syms.forEach(function (s) {
      const node = byQ[s.qualname];
      const dot = s.qualname.lastIndexOf(".");
      const parent = dot > 0 ? byQ[s.qualname.slice(0, dot)] : null;
      if (parent && parent !== node) parent.kids.push(node);
      else tops.push(node);
    });
    return { tops: tops, byQ: byQ };
  }

  function emitNode(rows, node, d) {
    const s = node.sym;
    const open = st().syms.has(s.card_id);
    rows.push({
      kind: "sym",
      depth: d,
      card: s.card_id,
      module: s.module,
      qual: s.qualname,
      kindName: s.kind,
      kids: node.kids.length,
      open: open,
      isEntry: !!s.is_entry,
      line: s.def_line || 0,
    });
    if (open) node.kids.forEach((k) => emitNode(rows, k, d + 1));
  }

  function pushNested(rows, module, syms, depth) {
    const nested = nestSyms(syms);
    nested.tops.forEach((n) => emitNode(rows, n, depth));
  }

  // ── 目录树 ──────────────────────────────────────────────────────
  // 树上三层：文件夹 → 文件 → 定义。文件夹按相对路径切出来，文件挂在
  // 它所在的目录下，函数和类仍然按点号嵌套在文件里。
  function buildDirTree(items) {
    const root = { name: "", dirs: new Map(), files: [] };
    items.forEach(function (it) {
      const path = it.path || it.module;
      const parts = path.split("/");
      let node = root;
      for (let i = 0; i < parts.length - 1; i++) {
        const name = parts[i];
        if (!node.dirs.has(name)) {
          node.dirs.set(name, { name: name, dirs: new Map(), files: [] });
        }
        node = node.dirs.get(name);
      }
      node.files.push({
        module: it.module,
        path: path,
        third: !!it.third,
        symbols: it.symbols || [],
      });
    });
    // 目录一个都不展开的话，进来的第一眼只是一排文件夹，看不到函数
    const nodes = new Map();
    (function walk(node, prefix) {
      nodes.set(prefix, node);
      node.dirs.forEach(function (child, name) {
        walk(child, prefix ? prefix + "/" + name : name);
      });
    })(root, "");
    return { root: root, nodes: nodes };
  }

  function emitDir(node, prefix, depth, rows) {
    const names = Array.from(node.dirs.keys()).sort(function (a, b) {
      return a.toLowerCase() < b.toLowerCase() ? -1 : 1;
    });
    names.forEach(function (name) {
      const child = node.dirs.get(name);
      const key = prefix ? prefix + "/" + name : name;
      const open = st().dirs.has(key);
      rows.push({ kind: "dir", depth: depth, dir: key, name: name, open: open });
      if (open) emitDir(child, key, depth + 1, rows);
    });
    node.files
      .slice()
      .sort(function (a, b) {
        const x = basename(a.path).toLowerCase();
        const y = basename(b.path).toLowerCase();
        return x < y ? -1 : x > y ? 1 : 0;
      })
      .forEach(function (f) {
        // 入口视图不再有文件这一层：一行函数名，下面一行小字是文件名（当注释看）
        if (spView === "entry") {
          f.symbols.forEach(function (s) {
            rows.push({
              kind: "entry",
              depth: depth,
              card: s.card_id,
              module: f.module,
              qual: s.qualname,
              file: basename(f.path),
              line: s.def_line || 0,
            });
            rows.push({
              kind: "note",
              depth: depth,
              card: s.card_id,
              file: basename(f.path),
            });
          });
          return;
        }
        const open = st().files.has(f.module);
        rows.push({
          kind: "file",
          depth: depth,
          module: f.module,
          path: f.path,
          third: f.third,
          open: open,
        });
        if (open) pushNested(rows, f.module, f.symbols, depth + 1);
      });
  }

  function collectItems() {
    const items = [];
    if (spView === "entry") {
      const byModule = new Map();
      (spEntrySyms || []).forEach(function (s) {
        if (!byModule.has(s.module)) byModule.set(s.module, []);
        byModule.get(s.module).push(s);
      });
      byModule.forEach(function (syms, module) {
        const f = spFileByModule.get(module);
        items.push({
          module: module,
          path: (f && f.path) || module.replace(/\./g, "/") + ".py",
          third: !!(f && f.is_third_party),
          symbols: syms,
        });
      });
      return items;
    }
    (spFiles || []).forEach(function (f) {
      // 一条定义都没有的文件不进函数树
      if (!f.symbols) return;
      items.push({
        module: f.module,
        path: f.path || f.module,
        third: !!f.is_third_party,
        symbols: spCache.get(f.module) || [],
      });
    });
    return items;
  }

  function collectRows() {
    const rows = [];
    if (spView === "fav" || spView === "recent") {
      const ids = spView === "fav" ? rootData.favorites || [] : rootData.recent || [];
      ids.forEach(function (id) {
        const p = splitId(id);
        const f = spFileByModule.get(p.module);
        const path = (f && f.path) || p.module.replace(/\./g, "/") + ".py";
        rows.push({
          kind: "entry",
          depth: 0,
          card: id,
          module: p.module,
          qual: p.qualname,
          file: basename(path),
          line: 0,
        });
        // 平铺视图没有目录树给上下文，注释行直接显示整条相对路径
        rows.push({ kind: "note", depth: 0, card: id, file: path });
      });
      return rows;
    }
    spTree = buildDirTree(collectItems());
    emitDir(spTree.root, "", 0, rows);
    return rows;
  }

  // 展开与收起用统一的图标这套（同一 viewBox、1.5 线宽、currentColor），
  // 只写一个字形的话放大缩小都对不齐别的图标
  const CARET_SHUT = ICON.chevronRight;
  const CARET_OPEN = ICON.chevronDown;

  function spRowEl(r) {
    const el = document.createElement("div");
    el.className =
      "sp-row" + (r.kind === "note" ? " note" : "") + (r.third ? " third" : "");
    el.style.paddingLeft = 6 + r.depth * 11 + "px";
    el.dataset.depth = String(r.depth);
    if (r.kind === "dir") {
      el.dataset.dir = r.dir;
      el.title = r.dir;
      el.innerHTML =
        '<span class="caret" data-act="caret">' + (r.open ? CARET_OPEN : CARET_SHUT) + "</span>" +
        '<span class="ic">' + ICON.folder + "</span>" +
        '<span class="nm">' + esc(r.name) + "</span>";
      return el;
    }
    if (r.kind === "file") {
      el.dataset.mod = r.module;
      el.title = r.path;
      el.innerHTML =
        '<span class="caret" data-act="caret">' + (r.open ? CARET_OPEN : CARET_SHUT) + "</span>" +
        '<span class="nm">' + esc(basename(r.path)) + "</span>";
      return el;
    }

    const active = window.getActiveRoot ? window.getActiveRoot() : null;
    if (active && active === r.card) el.classList.add("sel");
    const star = favorites.has(r.card);
    const flagHtml =
      '<span class="flag ' + (star ? "on" : "off") + '" data-fav="' + esc(r.card) +
      '" title="标记入口">' + FLAG_SVG + "</span>";

    // 一行函数名，一行文件名当注释：一行硬塞两个名字在窄面板里必然被截断
    if (r.kind === "entry") {
      el.dataset.card = r.card;
      el.dataset.module = r.module || "";
      el.title = (r.file ? r.file : r.module) + ":" + showQual(r.qual);
      el.innerHTML =
        '<span class="caret"></span>' +
        '<span class="ic">\u0192</span>' +
        '<span class="nm fn">' +
        (r.hot && r.hotName ? markHit(shortName(r.card), r.hotName) : esc(shortName(r.card))) +
        "</span>" +
        '<span class="tail">' + flagHtml + "</span>";
      return el;
    }
    if (r.kind === "more") {
      el.dataset.more = "1";
      el.innerHTML = '<span>加载更多</span>';
      return el;
    }
    if (r.kind === "note") {
      // 文件名行缩进到函数名文字那一列：caret 10 + 间隙 5 + 图标 11 + 间隙 5
      // 点它跟点上面那行函数一样，打开同一个符号
      el.style.paddingLeft = 6 + r.depth * 11 + 31 + "px";
      el.dataset.card = r.card;
      el.innerHTML =
        '<span class="note-file">' +
        (r.hot && r.hotPath ? markHit(r.file, r.hotPath) : esc(r.file)) +
        "</span>";
      return el;
    }

    const caret = r.kids
      ? '<span class="caret" data-act="caret">' + (r.open ? CARET_OPEN : CARET_SHUT) + "</span>"
      : '<span class="caret"></span>';
    const ic = r.kindName === "class" ? "C" : r.kindName === "method" ? "M" : "\u0192";
    el.dataset.card = r.card;
    el.dataset.module = r.module || "";
    el.title = r.module + ":" + showQual(r.qual || "");
    el.innerHTML =
      caret +
      '<span class="ic">' + ic + "</span>" +
      '<span class="nm">' + esc(shortName(r.card)) + "</span>" +
      (r.isEntry ? '<span class="edot" title="自动识别的入口"></span>' : "") +
      '<span class="tail">' +
        (r.showModule ? '<span class="mod">' + esc(r.module) + "</span>" : "") +
        flagHtml +
      "</span>";
    return el;
  }

  // 行数可能到几万（索引了整个标准库那种），一屏只画需要的那几张，
  // 滚到底再补下一批：DOM 数量跟着视口走，不跟着数据量走。
  const SP_PAGE = 200;
  let spRows = [];
  let spDrawn = 0;

  function drawMore() {
    if (spDrawn >= spRows.length) return;
    const end = Math.min(spDrawn + SP_PAGE, spRows.length);
    const frag = document.createDocumentFragment();
    for (; spDrawn < end; spDrawn++) frag.appendChild(spRowEl(spRows[spDrawn]));
    spList.appendChild(frag);
  }

  spList.addEventListener("scroll", function () {
    if (spList.scrollTop + spList.clientHeight >= spList.scrollHeight - 400) drawMore();
  });

  async function spRender() {
    if (!sideOpen) return;
    const token = ++spSeq;
    if (spView === "entry") await ensureEntry();
    if (token !== spSeq) return;

    const rows = collectRows();
    spList.innerHTML = "";
    spRows = rows;
    spDrawn = 0;
    if (!rows.length) {
      const empty = document.createElement("div");
      empty.className = "sp-empty";
      empty.textContent =
        spView === "fav"
          ? "给定义按 ★ 就会出现在这里"
          : spView === "recent"
            ? "在卡片里操作过的函数会排在这里"
          : spView === "entry"
            ? "扫描器没有找到入口"
            : "没有定义";
      spList.appendChild(empty);
      return;
    }
    drawMore();
  }

  // 展开收起就地增删那几行，不重画整棵树：一个文件几百行定义时，
  // 每点一次都重建全部 DOM 会把展开变成肉眼可见的卡顿。
  function dropChildren(rowEl, keepDepth) {
    let n = rowEl.nextElementSibling;
    while (n && Number(n.dataset.depth) > keepDepth) {
      const next = n.nextElementSibling;
      n.remove();
      n = next;
    }
  }

  // 插入也分批：一个大目录里可能有好几百个文件，一次 append 完会让点击
  // 到画面更新之间空一拍。每帧补一批，插完为止。
  function appendRows(anchor, rows) {
    let i = 0;
    let last = anchor;
    function step() {
      // 锚点被移除（视图切了、父节点收起了）就停下来，别往野节点里塞
      if (!last.isConnected) return;
      const frag = document.createDocumentFragment();
      const end = Math.min(i + SP_PAGE, rows.length);
      for (; i < end; i++) frag.appendChild(spRowEl(rows[i]));
      const tail = frag.lastChild;
      last.after(frag);
      last = tail;
      if (i < rows.length) requestAnimationFrame(step);
    }
    step();
  }

  function toggleDir(key) {
    const mine = st();
    const wasOpen = mine.dirs.has(key);
    if (wasOpen) mine.dirs.delete(key);
    else mine.dirs.add(key);
    const el = spList.querySelector('[data-dir="' + key + '"]');
    if (!el || !spTree || !spTree.nodes.has(key)) {
      spRender();
      return;
    }
    const depth = Number(el.dataset.depth);
    dropChildren(el, depth);
    el.querySelector('[data-act="caret"]').innerHTML = wasOpen
      ? CARET_SHUT
      : CARET_OPEN;
    if (wasOpen) return;
    const rows = [];
    emitDir(spTree.nodes.get(key), key, depth + 1, rows);
    appendRows(el, rows);
  }

  // 取定义是异步的，展开期间再点同一条就等它插完再说，否则上一次的回包
  // 会把已经收起的行又塞回来
  const spBusy = new Set();

  async function toggleFile(module) {
    if (spBusy.has(module)) return;
    const mine = st();
    const wasOpen = mine.files.has(module);
    if (wasOpen) mine.files.delete(module);
    else mine.files.add(module);
    const el = spList.querySelector('[data-mod="' + module + '"]');
    if (!el) {
      spRender();
      return;
    }
    const depth = Number(el.dataset.depth);
    dropChildren(el, depth);
    el.querySelector('[data-act="caret"]').innerHTML = wasOpen
      ? CARET_SHUT
      : CARET_OPEN;
    if (wasOpen) return;

    spBusy.add(module);
    try {
      const syms = spView === "entry" && spEntrySyms
        ? spEntrySyms.filter((s) => s.module === module)
        : await ensureSyms(module);
      // 取数期间可能已经点了别处、重渲染过，甚至又点了这一行
      if (!mine.files.has(module)) return;
      const host = spList.querySelector('[data-mod="' + module + '"]');
      if (!host) return;
      const rows = [];
      pushNested(rows, module, syms || [], Number(host.dataset.depth) + 1);
      appendRows(host, rows);
    } finally {
      spBusy.delete(module);
    }
  }

  function toggleSym(card) {
    const mine = st();
    const wasOpen = mine.syms.has(card);
    if (wasOpen) mine.syms.delete(card);
    else mine.syms.add(card);
    const el = spList.querySelector('[data-card="' + card + '"]');
    if (!el) {
      spRender();
      return;
    }
    const depth = Number(el.dataset.depth);
    dropChildren(el, depth);
    el.querySelector('[data-act="caret"]').innerHTML = wasOpen
      ? CARET_SHUT
      : CARET_OPEN;
    if (wasOpen) return;
    // 只把这条定义自己的下一层挂上去；拿整份文件重算再截取的话，
    // 末尾会把它的兄弟节点一起塞进来
    const module = el.dataset.module || "";
    const qual = splitId(card).qualname;
    const syms =
      spView === "entry" && spEntrySyms
        ? spEntrySyms.filter((s) => s.module === module)
        : spCache.get(module) || [];
    const node = nestSyms(syms).byQ[qual];
    if (!node) return;
    const rows = [];
    node.kids.forEach((k) => emitNode(rows, k, depth + 1));
    appendRows(el, rows);
  }

  // 浏览器预览没有后端，用导出的卡片凑一棵树
  function previewFiles() {
    const cards = (window.__CLEW__ && window.__CLEW__.cards) || {};
    const byModule = Object.create(null);
    const out = [];
    Object.keys(cards).forEach(function (id) {
      const c = cards[id] || {};
      const p = splitId(id);
      let f = byModule[p.module];
      if (!f) {
        f = byModule[p.module] = {
          module: p.module,
          path: p.module.replace(/\./g, "/") + ".py",
          is_third_party: false,
          has_main_guard: false,
          symbols: 0,
        };
        out.push(f);
      }
      f.symbols += 1;
      if (!spCache.has(p.module)) spCache.set(p.module, []);
      spCache.get(p.module).push({
        card_id: id,
        module: p.module,
        qualname: p.qualname,
        kind: c.kind || "func",
        def_line: (c.lines && c.lines[0] && c.lines[0].ln) || 0,
        is_entry: false,
        params: [],
        fanin: 0,
      });
    });
    const entries = Object.create(null);
    (window.__CLEW_ENTRIES__ || []).forEach(function (e) {
      if (e && e.id) entries[e.id] = true;
    });
    spEntrySyms = [];
    out.forEach(function (f) {
      f.symbols.sort(function (a, b) {
        return a.def_line - b.def_line;
      });
      (spCache.get(f.module) || []).forEach(function (s) {
        if (entries[s.card_id]) {
          s.is_entry = true;
          spEntrySyms.push(s);
        }
      });
    });
    spEntrySyms.sort(function (a, b) {
      return a.module < b.module ? -1 : a.module > b.module ? 1 : a.def_line - b.def_line;
    });
    out.sort(function (a, b) {
      return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
    });
    return out;
  }

  function spReset() {
    spCache.clear();
    spFileByModule = new Map();
    spStates.all = spBlank();
    spStates.entry = spBlank();
    spStates.fav = spBlank();
    spStates.recent = spBlank();
    spTree = null;
    spEntrySyms = null;
  }

  async function spLoad() {
    if (!desktop) {
      if (spLoadedFor !== "preview") {
        spReset();
        spFiles = previewFiles();
        spFiles.forEach((f) => spFileByModule.set(f.module, f));
        spLoadedFor = "preview";
      }
      spRender();
      return;
    }
    if (!currentPath) return;
    if (spLoadedFor === currentPath) {
      spRender();
      return;
    }
    spLoadedFor = null;
    try {
      const rows = await api.invoke("files");
      spReset();
      spFiles = rows || [];
      spFiles.forEach((f) => spFileByModule.set(f.module, f));
      spLoadedFor = currentPath;
    } catch (err) {
      // 索引还没建好时这里会失败，等建完 watchScan 会再叫一次
      if (window.__clewLog) window.__clewLog("files 取数失败：" + err);
      if (!spFiles) spFiles = [];
    }
    spRender();
  }

  function setSideOpen(on) {
    sideOpen = on;
    side.hidden = !on;
    document.body.classList.toggle("side-open", on);
    if (on) {
      applyTab();
      spLoad();
    } else {
      // 面板收起来了，活动栏就不该还亮着一个 tab
      Array.prototype.forEach.call(rail.children, function (b) {
        b.classList.remove("on");
      });
    }
    // 画布宽度变了，卡片要重排；app.js 只监听 resize
    window.dispatchEvent(new Event("resize"));
  }

  function spSearchReset() {
    clearTimeout(spSearch.timer);
    spSearch.timer = setTimeout(function () {
      spSearch.hits = [];
      spSearch.total = 0;
      spSearchPage(false);
    }, 250);
  }
  spQPath.addEventListener("input", spSearchReset);
  spQName.addEventListener("input", spSearchReset);

  // ── 活动栏 tab ──────────────────────────────────────────────────
  let sideTab = "tree";

  function applyTab() {
    Array.prototype.forEach.call(rail.children, function (b) {
      b.classList.toggle("on", b.dataset.tab === sideTab);
    });
    spPaneTree.hidden = sideTab !== "tree";
    spPaneSearch.hidden = sideTab !== "search";
    if (sideTab === "search" && !spSearch.hits.length && !spSearch.total) {
      spSearchPage(false);
    }
  }

  rail.addEventListener("click", function (e) {
    const btn = e.target.closest("[data-tab]");
    if (!btn) return;
    const tab = btn.dataset.tab;
    // 点当前 tab = 收起面板；点另一个 = 切过去并展开
    if (sideOpen && tab === sideTab) {
      setSideOpen(false);
      return;
    }
    sideTab = tab;
    applyTab();
    setSideOpen(true);
  });

  // 收藏变化后的最小更新：三处视图各自认领，谁也不调谁
  favorites.subscribe(function (id) {
    const sel = '[data-fav="' + String(id).replace(/"/g, '\\"') + '"]';
    const on = favorites.has(id);
    [spList, spListSearch].forEach(function (host) {
      if (!host) return;
      Array.prototype.forEach.call(host.querySelectorAll(sel), function (el) {
        el.classList.toggle("on", on);
        el.classList.toggle("off", !on);
      });
    });
    // 收藏夹是个列表，条目会增删，只能重画
    if (spView === "fav") spRender();
    // 卡片头那颗星
    if (window.__clewRefreshCard) window.__clewRefreshCard(id);
  });

  async function onTreeClick(e) {
    const flag = e.target.closest("[data-fav]");
    if (flag) {
      e.stopPropagation();
      await toggleFavorite(flag.dataset.fav);
      // 树和搜索面板是两套列表，收藏状态两边都要重新画
      spRender();
      if (sideTab === "search") spSearchRender();
      return;
    }
    const row = e.target.closest(".sp-row");
    if (!row) return;
    if (row.dataset.dir) {
      toggleDir(row.dataset.dir);
      return;
    }
    if (row.dataset.mod) {
      toggleFile(row.dataset.mod);
      return;
    }
    if (!row.dataset.card) return;
    // 点箭头只管展开，点名字才换根，跟 PyCharm 里文件树的手感一致
    if (e.target.closest('[data-act="caret"]')) {
      toggleSym(row.dataset.card);
      return;
    }
    if (typeof window.openRoot === "function") await window.openRoot(row.dataset.card);
  }

  spList.addEventListener("click", onTreeClick);

  // 搜索面板：结果行的点击规则和树一致（同一份处理），再加上「加载更多」
  spListSearch.addEventListener("click", async function (e) {
    const more = e.target.closest("[data-more]");
    if (more) {
      spSearchPage(true);
      return;
    }
    await onTreeClick(e);
  });

  // 换根之后把高亮挪到新的一行，不重取数据
  const rootChangedBefore = window.__clewRootChanged;
  window.__clewRootChanged = function (rootId) {
    if (rootChangedBefore) rootChangedBefore(rootId);
    if (!sideOpen) return;
    const list = spList.querySelectorAll(".sp-row.sel");
    Array.prototype.forEach.call(list, function (el) {
      el.classList.remove("sel");
    });
    if (!rootId) return;
    Array.prototype.forEach.call(spList.querySelectorAll(".sp-row"), function (el) {
      if (el.dataset.card === rootId) el.classList.add("sel");
    });
  };

  // 打开/关闭项目后由外壳调用一次：换项目就重取，回欢迎页就收起。
  function spSyncProject() {
    if (!currentPath) {
      spFiles = null;
      spLoadedFor = null;
      spReset();
      setSideOpen(false);
      return;
    }
    if (!sideOpen) return; // 收着的侧栏不该在后台白取一份数据
    spLoad();
  }
  window.__clewSide = {
    toggle: setSideOpen,
    reload: function () {
      spLoadedFor = null;
      spLoad();
    },
    setView: setView,
    sync: spSyncProject,
    rows: function () {
      return collectRows();
    },
  };

  syncViewLabel();

  // 启动时已经在项目里就直接把侧栏亮出来；欢迎页上它是收起的
  if (currentPath || !desktop) {
    setSideOpen(true);
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








