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

    /* 顶栏的根按钮 */
    #root-btn{display:flex;align-items:center;gap:7px;max-width:300px;flex:none;
      background:#1a2029;border:1px solid var(--bd);border-radius:6px;height:24px;
      padding:0 9px;cursor:pointer;font:inherit;font-size:12px;color:var(--tx)}
    #root-btn:hover{border-color:var(--acc);background:#1d2530}
    #root-btn.open{border-color:var(--acc);box-shadow:0 0 0 2px rgba(88,166,255,.18)}
    #root-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;
      max-width:236px;color:#c6d0da}
    #root-star{color:#e3b341;opacity:.35;font-size:11px}
    #root-star.on{opacity:1}
    .rb-caret{color:var(--dim);font-size:9px}

    /* 入口面板：上排自定义入口与最近，中间搜索带种类勾选，下排符号与文件结果 */
    #entry-panel{position:fixed;left:12px;top:88px;width:min(860px,calc(100vw - 24px));
      height:min(660px,calc(100vh - 108px));
      background:#161b22;border:1px solid #30363d;border-radius:10px;
      box-shadow:0 24px 60px #000b;display:flex;flex-direction:column;z-index:60;overflow:hidden}
    #entry-panel[hidden]{display:none}
    .ep-top{display:flex;flex:0 0 auto;max-height:38%;min-height:0;background:#141920;
      border-bottom:1px solid #30363d}
    .ep-cols{display:flex;flex:1;min-height:0}
    .ep-col-l{flex:0 0 42%;width:42%;border-right:1px solid #262c34;
      display:flex;flex-direction:column;min-height:0}
    .ep-col-r{flex:1;min-width:0;display:flex;flex-direction:column;min-height:0}
    /* 搜索带是下排的抬头：底色比上排亮一档，和下面两块连成一组 */
    .ep-bar{display:flex;align-items:center;gap:10px;padding:8px 12px;flex:none;
      border-bottom:1px solid #20262e;background:transparent}
    .ep-bar input{flex:1;min-width:0;background:#1a2029;border:1px solid #2a313a;
      border-radius:5px;height:26px;padding:0 9px;color:var(--tx);font:inherit;
      font-size:12px;outline:0}
    .ep-bar input::placeholder{color:#525b66}
    .ep-bar input:focus{border-color:#3d4f66;background:#1d2530}
    .ep-kinds{display:flex;gap:9px;font-size:11.5px;color:#8b95a2;flex:none}
    .ep-kinds label{display:flex;align-items:center;gap:3px;cursor:pointer;
      user-select:none;-webkit-user-select:none}
    .ep-kinds label:hover{color:#c6d0da}
    .ep-kinds input{margin:0;accent-color:var(--acc);width:12px;height:12px}
    .ep-filt{padding:6px 12px;color:#6b7480;font-size:11px}
    .ep-more{color:var(--acc);cursor:pointer}
    .ep-more:hover{text-decoration:underline}
    .ep-head{display:flex;align-items:center;gap:7px;padding:9px 12px 7px;
      color:#8b95a2;font-size:11px;letter-spacing:.5px;flex:none}
    .ep-head .n{color:#4a525c}
    .ep-sf{display:flex;align-items:center;gap:7px;padding:0 12px 8px;
      border-bottom:1px solid #262c34;flex:none}
    .ep-sf input{flex:1;min-width:0;background:#1a2029;border:1px solid #2a313a;border-radius:5px;
      height:24px;padding:0 8px;color:var(--tx);font:inherit;font-size:12px;outline:0}
    .ep-sf input::placeholder{color:#525b66}
    .ep-sf input:focus{border-color:#3d4f66;background:#1d2530}
    .ep-scope{color:#5b6472;font-size:9.5px;white-space:nowrap}
    .ep-list{overflow:auto;padding:2px 0 6px;flex:1;min-height:0}
    .ep-list::-webkit-scrollbar{width:9px}
    .ep-list::-webkit-scrollbar-track{background:transparent}
    .ep-list::-webkit-scrollbar-thumb{background:#2d333b;border-radius:5px;
      border:3px solid transparent;background-clip:content-box}
    .ep-list::-webkit-scrollbar-thumb:hover{background:#4a525c;background-clip:content-box}
    .ep-sect{padding:7px 12px 3px;display:flex;align-items:center;gap:6px;
      color:#6b7480;font-size:10.5px;letter-spacing:.6px}
    .ep-sect .n{color:#4a525c}
    /* 分组标题里也要能看到命中的那一段，字间距跟着周围走，别单独归零 */
    .ep-sect b{color:#ffd479;font-weight:600;
      background:rgba(255,212,121,.10);border-radius:2px}
    .ep-sect .act{margin-left:auto;color:#5b6472;cursor:pointer;letter-spacing:0;font-size:10px}
    .ep-sect .act:hover{color:var(--acc)}
    .ep-grp{display:flex;align-items:center;gap:6px;padding:5px 12px 2px;margin-top:1px;
      color:#8b95a2;font-size:10.5px;background:#171d24;border-top:1px solid #1e242c;
      cursor:pointer}
    .ep-grp .n{color:#4a525c}
    .ep-grp:hover{background:#1b222c}
    .ep-grp .arw{width:11px;flex:none;color:#5b6472;font-size:9px;text-align:center}
    .ep-grp .nm{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
    .ep-grp .hitmark{color:#ffd479;font-size:10px;margin-left:6px;letter-spacing:0}
    .ep-grp .tail .ln{max-width:210px;overflow:hidden;text-overflow:ellipsis;cursor:pointer}
    .ep-grp .tail .ln:hover{color:var(--acc)}
    .ep-node{display:flex;align-items:center;gap:5px;padding:2px 14px 2px 0;cursor:pointer;
      font-size:12px;white-space:nowrap;border-left:2px solid transparent}
    .ep-node:hover{background:#1d242e}
    .ep-node .arw{width:11px;flex:none;color:#5b6472;font-size:9px;text-align:center}
    .ep-node .ic{width:14px;flex:none;text-align:center;color:#5b6472;font-size:11px}
    .ep-node.file .ic{color:#58a6ff;opacity:.75}
    .ep-node.sym .ic{color:#f0883e;opacity:.85}
    .ep-node .nm{overflow:hidden;text-overflow:ellipsis;min-width:0;flex:1}
    .ep-node .tail{display:flex;align-items:center;gap:7px;flex:none;margin-left:auto}
    .ep-node .ln,.ep-node .cnt{flex:none;color:#4a525c;font-size:10px}
    .ep-node .flag{flex:none;color:#3c454f;padding:2px 3px;border-radius:3px;
      display:inline-flex;align-items:center;gap:3px;line-height:0}
    .ep-node:hover .flag{color:#8b95a2}
    .ep-node .flag.on{color:#e3b341}
    .ep-node .flag:hover{background:#2b2416;color:#ffd479}
    .ep-node .flag svg{display:block}
    .ep-node b{color:#ffd479;font-weight:600;background:rgba(255,212,121,.10);border-radius:2px}
    .ep-dot{width:5px;height:5px;border-radius:3px;background:#f0883e;flex:none;opacity:.8}
    .ep-empty{padding:8px 12px;color:#5f6874;font-size:11.5px}
    /* 尾部放所在容器：符号给模块、文件给目录、模块给父模块，长了截断 */
    .ep-node .ln{max-width:210px;overflow:hidden;text-overflow:ellipsis}
    .ep-foot{border-top:1px solid #262c34;padding:6px 12px;color:#4a525c;font-size:10.5px;
      display:flex;gap:14px;flex:none;background:#141920}
    .ep-foot .k{color:#6b7480}
    .ep-foot .right{margin-left:auto;color:#e3b341}
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

  const rootBtn = document.createElement("button");
  rootBtn.id = "root-btn";
  rootBtn.title = "选择从这个函数开始读";
  rootBtn.innerHTML =
    '<span id="root-star">★</span><span id="root-name">未选择入口</span>' +
    '<span class="rb-caret">▾</span>';

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
  row2.appendChild(rootBtn);
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

  function enterProject(path) {
    currentPath = normPath(path);
    remember(path);
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
    renderProjectButton();
    setStatus("");
    showWelcome();
  }

  // ── 启动 ────────────────────────────────────────────────────────
  if (currentPath) remember(currentPath);
  renderProjectButton();

  // 命令行带进来的项目若还没索引，一样先进界面再等索引
  const bootInfo = window.__CLEW_BOOT_INFO__;
  if (bootInfo && bootInfo.state === "indexing") {
    hideWelcome();
    watchScan(bootInfo.root);
  } else {
    // 预览模式没有项目路径，只要图里有入口就直接进画布
    const hasGraph = typeof ENTRIES !== "undefined" && ENTRIES.length > 0;
    if ((currentPath || !desktop) && hasGraph) {
      hideWelcome();
    } else {
      showWelcome();
    }
  }

  // ── 入口面板 ────────────────────────────────────────────────────
  // 四块：上面自定义入口与最近入口常驻，中间一条输入带种类勾选，下面符号结果
  // 与文件结果。空查询时下面两块退回扫描入口与文件树。搜索由后端统一打分，
  // 前端只按种类分队渲染。
  const FLAG_SVG =
    '<svg viewBox="0 0 12 12" width="12" height="12" ' +
    'style="position:static;inset:auto;width:12px;height:12px;display:block">' +
    '<path d="M2.2 1.1v9.8" stroke="currentColor" stroke-width="1.1" fill="none"/>' +
    '<path d="M2.2 2h7.3L7.7 4.2l1.8 2.2H2.2z" fill="currentColor"/></svg>';

  // 面板里的图标统一从这里出，尺寸写成 inline，免得被页面那条 svg 全局规则撑开
  function panelIcon(body) {
    return (
      '<svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor" ' +
      'stroke-width="1.15" stroke-linecap="round" stroke-linejoin="round" ' +
      'style="position:static;inset:auto;width:12px;height:12px;display:block">' +
      body +
      "</svg>"
    );
  }
  const ICON_FOLDER = panelIcon('<path d="M1.5 3.3h3.1l1 1.2h4.9v5.9H1.5z"/>');
  const ICON_FOLDER_OPEN =
    panelIcon('<path d="M1.5 3.3h3.1l1 1.2h4.9v1.2H1.5z"/><path d="M1.5 5.7h9.4l-1.3 4.7H1.5z"/>');
  const ICON_FILE =
    panelIcon('<path d="M3.1 1.6h3.4l2.4 2.4v6.4H3.1z"/><path d="M6.5 1.6v2.4h2.4"/>');

  const KIND_LABELS = [
    ["func", "函数"],
    ["method", "方法"],
    ["class", "类"],
    ["file", "文件"],
    ["mod", "模块"],
  ];
  const KINDS_KEY = "clew.searchKinds";

  const panel = document.createElement("div");
  panel.id = "entry-panel";
  panel.hidden = true;
  panel.innerHTML =
    '<div class="ep-top">' +
      '<div class="ep-col-l"><div class="ep-head">自定义入口<span class="n" id="ep-fav-count"></span></div>' +
        '<div class="ep-list" id="ep-fav-list"></div></div>' +
      '<div class="ep-col-r"><div class="ep-head">最近<span class="n" id="ep-recent-count"></span></div>' +
        '<div class="ep-list" id="ep-recent-list"></div></div>' +
    "</div>" +
    '<div class="ep-bar">' +
      '<input id="ep-q" placeholder="搜索 函数 / 方法 / 类 / 文件 / 模块" autocomplete="off">' +
      '<div class="ep-kinds">' +
        KIND_LABELS.map(
          ([k, label]) =>
            '<label title="Ctrl+' + (KIND_LABELS.findIndex((x) => x[0] === k) + 1) + '">' +
            '<input type="checkbox" data-kind="' + k + '">' + label + "</label>"
        ).join("") +
      "</div>" +
    "</div>" +
    '<div class="ep-cols">' +
      '<div class="ep-col-l ep-left"><div class="ep-head">符号<span class="n" id="ep-sym-count"></span></div>' +
        '<div class="ep-list" id="ep-sym-list"></div></div>' +
      '<div class="ep-col-r"><div class="ep-head">文件与模块<span class="n" id="ep-path-count"></span></div>' +
        '<div class="ep-list" id="ep-path-list"></div></div>' +
    "</div>" +
    '<div class="ep-foot">' +
      '<span><span class="k">↵</span> 设为根</span>' +
      '<span><span class="k">⚑</span> 标记入口</span>' +
      '<span><span class="k">Ctrl+1..5</span> 种类</span>' +
      '<span class="right" id="ep-marked"></span>' +
    "</div>";
  document.body.appendChild(panel);

  const qInput = panel.querySelector("#ep-q");
  const favEntryList = panel.querySelector("#ep-fav-list");
  const recentEntryList = panel.querySelector("#ep-recent-list");
  const symHitList = panel.querySelector("#ep-sym-list");
  const pathHitList = panel.querySelector("#ep-path-list");
  const kindBoxes = Array.from(panel.querySelectorAll("[data-kind]"));

  let panelOpen = false;
  let fileCache = null;              // files() 的结果，懒加载一次
  let fileByModule = new Map();      // module -> FileRow
  let symCache = new Map();          // module -> SymbolRow[]
  let rootData = { favorites: [], recent: [] };
  let searchSeq = 0;
  let hits = { syms: [], paths: [], suppressed: [] };
  const openDirs = new Set();
  const openFiles = new Set();
  const openGroups = new Set();
  const collapsedFiles = new Set();   // 结果里被收起的文件组

  // 勾选状态落盘，重开软件不用再拨一遍
  const kinds = { func: true, method: true, class: true, file: true, mod: true };
  try {
    const saved = JSON.parse(localStorage.getItem(KINDS_KEY) || "null");
    if (saved) KIND_LABELS.forEach(([k]) => { kinds[k] = saved[k] !== false; });
  } catch (err) {
    /* 读不出来就全选 */
  }
  function saveKinds() {
    try {
      localStorage.setItem(KINDS_KEY, JSON.stringify(kinds));
    } catch (err) {
      /* 隐私模式写不进就算了 */
    }
  }
  function syncBoxes() {
    kindBoxes.forEach((b) => { b.checked = kinds[b.dataset.kind] !== false; });
  }
  function enabledKinds() {
    return KIND_LABELS.map(([k]) => k).filter((k) => kinds[k]);
  }

  function esc(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }
  function hl(text) {
    return esc(text);
  }
  /// 后端给的是字节偏移，纯 ASCII 时与 JS 下标一致，否则放弃上色
  function markSpans(text, spans) {
    if (!spans || !spans.length) return esc(text);
    let bytes = 0;
    for (let i = 0; i < text.length; i++) bytes += text.charCodeAt(i) < 0x80 ? 1 : 3;
    if (bytes !== text.length) return esc(text);
    let out = "";
    let last = 0;
    spans.forEach(function (pair) {
      const s = pair[0];
      const e = pair[1];
      if (s < last || e > text.length) return;
      out += esc(text.slice(last, s)) + "<b>" + esc(text.slice(s, e)) + "</b>";
      last = e;
    });
    return out + esc(text.slice(last));
  }
  function splitId(id) {
    const i = id.indexOf(":");
    return i < 0
      ? { module: id, qualname: id }
      : { module: id.slice(0, i), qualname: id.slice(i + 1) };
  }
  function shortName(id) {
    const parts = splitId(id);
    if (parts.qualname === "<module>") {
      return parts.module.split(".").pop() || parts.module;
    }
    return parts.qualname.split(".").pop() || parts.qualname;
  }
  function fileNameOf(module) {
    const f = fileByModule.get(module);
    if (f) return f.path.split("/").pop();
    return (module.split(".").pop() || module) + ".py";
  }
  function scannedEntries() {
    return (typeof ENTRIES !== "undefined" ? ENTRIES : []).filter((e) => e && e.id);
  }
  function activeModule() {
    const id = window.getActiveRoot ? window.getActiveRoot() : null;
    return id ? splitId(id).module : null;
  }

  // 一行入口：名字、文件、旗标。入口栏不参与打分，只按原样列。
  function entryRow(id) {
    const star = rootData.favorites.indexOf(id) >= 0;
    const isEntry = scannedEntries().some((e) => e.id === id);
    return (
      '<div class="ep-node sym" data-root="' + esc(id) + '">' +
        '<span class="ic">\u0192</span>' +
        '<span class="nm">' + hl(shortName(id)) + "</span>" +
        (isEntry ? '<span class="ep-dot" title="扫描器认定的入口"></span>' : "") +
        '<span class="tail">' +
          '<span class="ln" title="' + esc(fileNameOf(splitId(id).module)) + '">' +
            esc(splitId(id).module) + "</span>" +
          '<span class="flag ' + (star ? "on" : "off") + '" data-fav="' + esc(id) +
            '" title="标记入口">' + FLAG_SVG + "</span>" +
        "</span>" +
      "</div>"
    );
  }

  // 一行搜索结果。符号可以设为根，文件与模块点开回到文件树里那一行。
  //
  // 名字只放最后一段，尾部放它所在的容器，这样同名的 main 能分清：
  // 符号给模块与行号，文件给目录，模块给父模块。命中的高亮永远落在名字上，
  // 而名字和 spans 都按最后一段算，两者才对得上。
  function hitRow(h, indent) {
    const isSymbol = h.kind === "func" || h.kind === "method" || h.kind === "class";
    const pad = indent ? ' style="padding-left:' + indent + 'px"' : "";
    const star = isSymbol && rootData.favorites.indexOf(h.card_id) >= 0;
    const label = isSymbol
      ? (h.qualname.split(".").pop() || h.qualname)
      : h.kind === "file"
        ? (h.path.split("/").pop() || h.path)
        : (h.module.split(".").pop() || h.module);
    const icon = h.kind === "class" ? "C" : h.kind === "file" ? ICON_FILE : h.kind === "mod" ? "\u25a4" : "\u0192";
    const text = h.field === "name" ? markSpans(label, h.spans) : esc(label);
    const dir = h.path.split("/").slice(0, -1).join("/");
    const where = isSymbol
      ? (indent ? "L" + h.def_line : h.module + ":" + h.def_line)
      : h.kind === "file"
        ? (dir || ".")
        : (h.module.split(".").slice(0, -1).join(".") ||
           (h.path.split("/").pop() || dir || "."));
    const tailTitle = isSymbol ? h.path + ":" + h.def_line : h.path;
    const attrs = isSymbol
      ? ' data-root="' + esc(h.card_id) + '"'
      : ' data-open="' + esc(h.module) + '"';
    return (
      '<div class="ep-node sym" data-kindrow="' + h.kind + '"' + attrs + pad + ">" +
        '<span class="ic">' + icon + "</span>" +
        '<span class="nm">' + text + "</span>" +
        '<span class="tail">' +
          '<span class="ln" title="' + esc(tailTitle) + '">' + esc(where) + "</span>" +
          (isSymbol
            ? '<span class="flag ' + (star ? "on" : "off") + '" data-fav="' + esc(h.card_id) +
              '" title="标记入口">' + FLAG_SVG + "</span>"
            : "") +
        "</span>" +
      "</div>"
    );
  }

  function scanGroups(rows) {
    const groups = new Map();
    rows.forEach((r) => {
      const top = splitId(r.id).module.split(".")[0] || "其他";
      if (!groups.has(top)) groups.set(top, []);
      groups.get(top).push(r);
    });
    return groups;
  }

  // 上左：自定义入口
  function paintFavs() {
    const favs = rootData.favorites;
    favEntryList.innerHTML = favs.length
      ? favs.map((id) => entryRow(id)).join("")
      : '<div class="ep-empty">给函数按 ⚑ 就会出现在这里</div>';
    panel.querySelector("#ep-fav-count").textContent = String(favs.length);
  }

  // 上右：最近
  function paintRecent() {
    const recent = rootData.recent.filter((id) => rootData.favorites.indexOf(id) < 0);
    recentEntryList.innerHTML = recent.length
      ? recent.map((id) => entryRow(id)).join("")
      : '<div class="ep-empty">打开过的入口按时间排在这里</div>';
    panel.querySelector("#ep-recent-count").textContent = String(recent.length);
  }

  function paintIdleSyms() {
    const scanned = scannedEntries();
    if (!scanned.length) {
      symHitList.innerHTML = '<div class="ep-empty">扫描器没有找到入口，用上面的搜索挑一个</div>';
      panel.querySelector("#ep-sym-count").textContent = "0";
      return;
    }
    let html = "";
    const groups = scanGroups(scanned);
    if (!openGroups.size) {
      const first = groups.keys().next();
      if (!first.done) openGroups.add(first.value);
    }
    groups.forEach((rows, name) => {
      const open = openGroups.has(name);
      html += '<div class="ep-grp" data-group="' + esc(name) + '"><span>' +
        (open ? "▾" : "▸") + "</span>" + esc(name) +
        '<span class="n">' + rows.length + "</span></div>";
      if (open) html += rows.map((r) => entryRow(r.id)).join("");
    });
    symHitList.innerHTML = html;
    panel.querySelector("#ep-sym-count").textContent = String(scanned.length);
  }

  // 文件树
  function buildTree(rows) {
    const root = { dirs: new Map(), files: [] };
    rows.forEach((r) => {
      const parts = r.path.split("/");
      const name = parts.pop();
      let node = root;
      let prefix = "";
      parts.forEach((p) => {
        prefix = prefix ? prefix + "/" + p : p;
        if (!node.dirs.has(p)) node.dirs.set(p, { path: prefix, dirs: new Map(), files: [] });
        node = node.dirs.get(p);
      });
      node.files.push({ name: name, row: r });
    });
    return root;
  }

  function dirRow(node, depth) {
    const open = openDirs.has(node.path);
    return (
      '<div class="ep-node" data-dir="' + esc(node.path) + '" style="padding-left:' +
        (depth * 14 + 8) + 'px">' +
        '<span class="arw">' + (open ? "▾" : "▸") + "</span>" +
        '<span class="ic">' + (open ? ICON_FOLDER_OPEN : ICON_FOLDER) + "</span>" +
        '<span class="nm">' + esc(node.path.split("/").pop()) + "</span>" +
        '<span class="tail"><span class="cnt">' + countFiles(node) + "</span></span></div>"
    );
  }
  function countFiles(node) {
    let n = node.files.length;
    node.dirs.forEach((d) => { n += countFiles(d); });
    return n;
  }
  function fileRow(row, depth) {
    const open = openFiles.has(row.module);
    return (
      '<div class="ep-node file" data-file="' + esc(row.module) + '" style="padding-left:' +
        (depth * 14 + 8) + 'px">' +
        '<span class="arw">' + (open ? "▾" : "▸") + "</span>" +
        '<span class="ic">' + ICON_FILE + "</span>" +
        '<span class="nm">' + esc(row.path.split("/").pop()) + "</span>" +
        (row.has_main_guard ? '<span class="ep-dot" title="有 __main__ 块"></span>' : "") +
        '<span class="tail"><span class="cnt">' + row.symbols + "</span></span></div>"
    );
  }
  function symRow(s, depth) {
    const star = rootData.favorites.indexOf(s.card_id) >= 0;
    const isCls = s.kind === "class";
    return (
      '<div class="ep-node sym" data-root="' + esc(s.card_id) + '" style="padding-left:' +
        (depth * 14 + 8) + 'px">' +
        '<span class="arw"></span>' +
        '<span class="ic">' + (isCls ? "C" : "\u0192") + "</span>" +
        '<span class="nm">' + esc(shortName(s.card_id)) + "</span>" +
        (s.is_entry ? '<span class="ep-dot" title="扫描器认定的入口"></span>' : "") +
        '<span class="tail">' +
          '<span class="ln">L' + s.def_line + "</span>" +
          '<span class="flag ' + (star ? "on" : "off") + '" data-fav="' + esc(s.card_id) +
            '" title="标记入口">' + FLAG_SVG + "</span>" +
        "</span>" +
      "</div>"
    );
  }

  function paintTree() {
    const tree = buildTree(fileCache || []);
    const out = [];
    (function walk(node, depth) {
      node.dirs.forEach((child) => {
        out.push(dirRow(child, depth));
        if (openDirs.has(child.path)) walk(child, depth + 1);
      });
      node.files.forEach((f) => {
        out.push(fileRow(f.row, depth));
        if (openFiles.has(f.row.module)) {
          (symCache.get(f.row.module) || []).forEach((s) => out.push(symRow(s, depth + 1)));
        }
      });
    })(tree, 0);
    pathHitList.innerHTML = out.join("") || '<div class="ep-empty">这个项目没有 Python 文件</div>';
    panel.querySelector("#ep-path-count").textContent = String((fileCache || []).length);
  }

  // 被勾选挡掉的命中要说出来，否则会以为项目里没有
  function suppressedNote(kinds, names) {
    const items = hits.suppressed
      .filter(([k, n]) => names.indexOf(k) >= 0 && n > 0)
      .map(([k, n]) => {
        const label = (KIND_LABELS.find((x) => x[0] === k) || ["", k])[1];
        return '<span class="ep-more" data-allow="' + k + '">另有 ' + n + " 个" + label + "命中</span>";
      });
    return items.length
      ? '<div class="ep-filt">' + items.join(" · ") + "，点击放开</div>"
      : "";
  }

  // 右栏把命中按文件归组：文件本身命中就当组头，命中的函数挂在下面。
  // 只按种类分两栏的话，搜函数名右栏一片空白，搜索就白做了。
  function groupByFile(list) {
    const order = [];
    const map = new Map();
    list.forEach(function (h) {
      const key = h.module || h.path;
      let g = map.get(key);
      if (!g) {
        g = { key: key, module: h.module, path: h.path, best: h.score, syms: [] };
        map.set(key, g);
        order.push(g);
      }
      g.best = Math.max(g.best, h.score);
      if (h.kind === "func" || h.kind === "method" || h.kind === "class") g.syms.push(h);
    });
    order.forEach((g) => g.syms.sort((a, b) => a.def_line - b.def_line));
    order.sort((a, b) => b.best - a.best || a.path.localeCompare(b.path));
    return order;
  }

  function groupHead(g) {
    const collapsed = collapsedFiles.has(g.key);
    const name = g.path.split("/").pop() || g.path;
    const hitFile = g.best > 0 && g.syms.length === 0;
    return (
      '<div class="ep-grp grp-hit' + (collapsed ? " collapsed" : "") + '" data-grp="' + esc(g.key) + '">' +
        '<span class="arw">' + (collapsed ? "▸" : "▾") + "</span>" +
        '<span class="ic">' + ICON_FILE + "</span>" +
        '<span class="nm">' + esc(name) + (hitFile ? '<span class="hitmark">命中</span>' : "") + "</span>" +
        '<span class="n">' + (g.syms.length || "") + "</span>" +
        '<span class="tail" data-open="' + esc(g.module) + '" title="在文件树里展开">' +
          '<span class="ln">' + esc(g.path.split("/").slice(0, -1).join("/") || ".") + "</span></span>" +
      "</div>"
    );
  }

  function paintHits() {
    const syms = hits.syms;
    symHitList.innerHTML = syms.length
      ? syms.map((h) => hitRow(h)).join("") + suppressedNote("sym", ["func", "method", "class"])
      : '<div class="ep-empty">没有匹配的符号</div>' +
        suppressedNote("sym", ["func", "method", "class"]);

    const groups = groupByFile(hits.all);
    if (!groups.length) {
      pathHitList.innerHTML = '<div class="ep-empty">没有匹配的文件</div>' +
        suppressedNote("path", ["file", "mod"]);
    } else {
      let html = "";
      groups.forEach(function (g) {
        html += groupHead(g);
        if (collapsedFiles.has(g.key)) return;
        g.syms.forEach(function (s) {
          html += hitRow(s, 22);
        });
      });
      pathHitList.innerHTML = html + suppressedNote("path", ["file", "mod"]);
    }
    panel.querySelector("#ep-sym-count").textContent = String(syms.length);
    panel.querySelector("#ep-path-count").textContent = String(groups.length);
  }

  function paint() {
    paintFavs();
    paintRecent();
    if (qInput.value.trim()) paintHits();
    else {
      paintIdleSyms();
      paintTree();
    }
  }

  // 同一时刻只让一个搜索在跑。按键连打时后端要拿索引的锁，排队会让界面
  // 越来越慢，这里只留住最后一次输入，上一趟回来再发。
  let searchBusy = false;
  let searchPending = null;

  async function runSearch() {
    const q = qInput.value.trim();
    if (!desktop) return;
    if (!q) {
      hits = { all: [], syms: [], suppressed: [] };
      paint();
      return;
    }
    if (searchBusy) {
      searchPending = true;
      return;
    }
    searchBusy = true;
    const seq = ++searchSeq;
    try {
      const page = await api.invoke("search", {
        q: q,
        kinds: enabledKinds(),
        limit: 200,
        ctx: activeModule(),
      });
      if (seq === searchSeq) {
        const list = page.hits || [];
        hits = {
          all: list,
          syms: list.filter((h) => h.kind === "func" || h.kind === "method" || h.kind === "class"),
          suppressed: page.suppressed || [],
        };
        paintHits();
      }
    } catch (err) {
      if (seq === searchSeq) {
        symHitList.innerHTML = '<div class="ep-empty">' + esc(String(err)) + "</div>";
        pathHitList.innerHTML = "";
      }
    } finally {
      searchBusy = false;
      if (searchPending) {
        searchPending = null;
        runSearch();
      }
    }
  }

  let searchTimer = null;
  qInput.addEventListener("input", function () {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(runSearch, 200);
  });

  async function ensureSymbols(module) {
    if (symCache.has(module)) return symCache.get(module);
    let rows = [];
    try {
      rows = await api.invoke("symbols_in", { module: module });
    } catch (err) {
      rows = [];
    }
    symCache.set(module, rows);
    return rows;
  }

  async function openEntryPanel() {
    if (panelOpen) return;
    panelOpen = true;
    panel.hidden = false;
    rootBtn.classList.add("open");
    syncBoxes();
    if (desktop && currentPath) {
      try {
        rootData = await api.invoke("roots", { project: currentPath, action: "get" });
      } catch (err) {
        rootData = { favorites: [], recent: [] };
      }
      if (!fileCache) {
        try {
          fileCache = await api.invoke("files");
        } catch (err) {
          fileCache = [];
        }
        fileByModule = new Map();
        (fileCache || []).forEach((f) => fileByModule.set(f.module, f));
      }
    } else {
      fileCache = [];
    }
    paint();
    updateMarked();
    qInput.focus();
  }

  function closeEntryPanel() {
    panelOpen = false;
    panel.hidden = true;
    rootBtn.classList.remove("open");
  }

  function updateMarked() {
    panel.querySelector("#ep-marked").textContent =
      "已标记 " + rootData.favorites.length + " 个入口";
  }

  async function chooseRoot(id) {
    closeEntryPanel();
    if (typeof window.openRoot === "function") await window.openRoot(id);
  }

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
    paint();
    updateMarked();
    window.__clewRootChanged(window.getActiveRoot ? window.getActiveRoot() : null);
    if (window.__clewRefreshCard) window.__clewRefreshCard(id);
  }

  // 从搜索结果点开文件：清掉查询，在文件树里展开那一行
  async function revealFile(module) {
    const row = fileByModule.get(module);
    qInput.value = "";
    hits = { all: [], syms: [], suppressed: [] };
    if (row) {
      const parts = (row.path || "").split("/");
      parts.pop();
      let prefix = "";
      parts.forEach((p) => {
        prefix = prefix ? prefix + "/" + p : p;
        openDirs.add(prefix);
      });
      openFiles.add(module);
      await ensureSymbols(module);
    }
    paint();
    const el = pathHitList.querySelector('[data-file="' + module.replace(/"/g, '\\"') + '"]');
    if (el && el.scrollIntoView) el.scrollIntoView({ block: "center" });
  }

  // 卡片头上的旗标按钮走这两个口
  window.__clewIsFavorite = function (id) {
    return !!id && rootData.favorites.indexOf(id) >= 0;
  };
  window.__clewToggleFavorite = function (id) {
    return toggleFavorite(id);
  };
  // app.js 取不到卡片时用它说话，否则一次失败只剩一片空白，看不出发生了什么
  window.__clewNote = function (text, isError) {
    setNote(text, !!isError);
  };

  panel.addEventListener("click", async function (e) {
    const more = e.target.closest("[data-allow]");
    if (more) {
      kinds[more.dataset.allow] = true;
      saveKinds();
      syncBoxes();
      runSearch();
      return;
    }
    const fav = e.target.closest("[data-fav]");
    if (fav) {
      e.stopPropagation();
      await toggleFavorite(fav.dataset.fav);
      return;
    }
    const root = e.target.closest("[data-root]");
    if (root) {
      await chooseRoot(root.dataset.root);
      return;
    }
    const open = e.target.closest("[data-open]");
    if (open) {
      await revealFile(open.dataset.open);
      return;
    }
    const grp = e.target.closest("[data-grp]");
    if (grp) {
      const k = grp.dataset.grp;
      collapsedFiles.has(k) ? collapsedFiles.delete(k) : collapsedFiles.add(k);
      paintHits();
      return;
    }
    const group = e.target.closest("[data-group]");
    if (group) {
      const g = group.dataset.group;
      const wasOpen = openGroups.has(g);
      openGroups.clear();
      if (!wasOpen) openGroups.add(g);
      paintIdleSyms();
      return;
    }
    const file = e.target.closest("[data-file]");
    if (file) {
      const module = file.dataset.file;
      if (openFiles.has(module)) {
        openFiles.delete(module);
      } else {
        openFiles.add(module);
        await ensureSymbols(module);
      }
      paintTree();
      return;
    }
    const dir = e.target.closest("[data-dir]");
    if (dir) {
      const p = dir.dataset.dir;
      openDirs.has(p) ? openDirs.delete(p) : openDirs.add(p);
      paintTree();
    }
  });

  qInput.addEventListener("keydown", function (e) {
    if (e.key !== "Enter") return;
    e.preventDefault();
    const first = symHitList.querySelector("[data-root]");
    if (first) chooseRoot(first.dataset.root);
  });

  kindBoxes.forEach(function (box) {
    box.addEventListener("change", function () {
      kinds[box.dataset.kind] = box.checked;
      saveKinds();
      runSearch();
    });
  });

  // Ctrl+1..5 切换种类，手不用离开键盘
  document.addEventListener("keydown", function (e) {
    if (!panelOpen || !(e.ctrlKey || e.metaKey)) return;
    const n = parseInt(e.key, 10);
    if (!n || n < 1 || n > KIND_LABELS.length) return;
    e.preventDefault();
    const k = KIND_LABELS[n - 1][0];
    kinds[k] = !kinds[k];
    saveKinds();
    syncBoxes();
    runSearch();
  });

  rootBtn.addEventListener("click", function () {
    panelOpen ? closeEntryPanel() : openEntryPanel();
  });
  document.addEventListener("mousedown", function (e) {
    if (!panelOpen) return;
    if (panel.contains(e.target) || rootBtn.contains(e.target)) return;
    closeEntryPanel();
  });
  document.addEventListener("keydown", function (e) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "p") {
      e.preventDefault();
      panelOpen ? closeEntryPanel() : openEntryPanel();
    }
    if (e.key === "Escape" && panelOpen) closeEntryPanel();
  });

  // app.js 在切根和打开根之后回调这里，顶栏按钮跟着变
  window.__clewRootChanged = function (rootId) {
    document.getElementById("root-name").textContent = rootId || "未选择入口";
    document.getElementById("root-name").title = rootId || "";
    const star = document.getElementById("root-star");
    star.classList.toggle("on", !!rootId && rootData.favorites.indexOf(rootId) >= 0);
  };
  window.__clewRootOpened = function (rootId) {
    if (!desktop || !currentPath || !rootId) return;
    api.invoke("roots", { project: currentPath, action: "recent", cardId: rootId })
      .then(function (data) {
        rootData = data;
        window.__clewRootChanged(rootId);
        updateMarked();
      })
      .catch(function () {});
  };

  // app.js 先加载，它开根的时候这个回调还没定义，这里补一次同步
  if (typeof window.getActiveRoot === "function") {
    window.__clewRootChanged(window.getActiveRoot());
  }

  // 启动时先把收藏读出来，卡片头上的旗标才有正确的初始状态
  if (desktop && currentPath) {
    api.invoke("roots", { project: currentPath, action: "get" })
      .then(function (d) {
        rootData = d;
        window.__clewRootChanged(window.getActiveRoot());
      })
      .catch(function () {});
  }

  // 调试口：浏览器预览里没有后端，用它注入假数据验证面板渲染
  // 调试口：浏览器预览里没有后端，用它注入假数据验证面板渲染
  window.__clewPanel = {
    open: openEntryPanel,
    close: closeEntryPanel,
    paint: paint,
    search: runSearch,
    setFiles: function (rows) {
      fileCache = rows || [];
      fileByModule = new Map();
      fileCache.forEach(function (f) { fileByModule.set(f.module, f); });
      symCache = new Map();
    },
    setRoots: function (d) {
      rootData = d || { favorites: [], recent: [] };
    },
    setHits: function (page) {
      const list = (page && page.hits) || [];
      hits = {
        all: list,
        syms: list.filter((h) => h.kind === "func" || h.kind === "method" || h.kind === "class"),
        suppressed: (page && page.suppressed) || [],
      };
      paintHits();
    },
    inputs: { q: qInput, kinds: kinds },
  };

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








