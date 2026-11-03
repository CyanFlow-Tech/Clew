// 应用外壳：标题栏（logo / 菜单 / 项目下拉）、文件菜单、打开项目。
//
// 图本身仍然由 app.js 负责；这里只动 #top 那一层，通过全局 bootProject()
// 换项目。

(function shell() {
  const api = window.__TAURI__.core;

  const RECENT_KEY = "clew.recentProjects";

  // ── 样式 ────────────────────────────────────────────────────────
  const style = document.createElement("style");
  style.textContent = `
    /* 两行标题栏：上面是品牌与菜单，下面是入口与面包屑 */
    #top{height:72px!important;flex-direction:column!important;align-items:stretch!important;
      gap:0!important;padding:0 12px!important;overflow:visible!important}
    #graph{inset:72px 0 0 0!important}
    #tb-row1{display:flex;align-items:center;gap:8px;height:38px;flex:none}
    #tb-row2{display:flex;align-items:center;gap:8px;height:34px;flex:none;
      border-top:1px solid #1b2028}
    .tb-brand{color:var(--acc);font-weight:600;white-space:nowrap;letter-spacing:.5px;
      display:flex;align-items:center;gap:6px;flex:none}
    .tb-brand svg{position:static;inset:auto;width:15px;height:15px;pointer-events:none}
    .tb-icon{background:none;border:1px solid transparent;color:var(--tx);cursor:pointer;
      padding:1px 7px;border-radius:4px;font:inherit;line-height:1.5;flex:none}
    .tb-icon:hover{border-color:var(--bd);background:#1f2630}
    .tb-select{background:#1f2630;border:1px solid var(--bd);border-radius:4px;
      padding:2px 6px;color:var(--tx);font:inherit;font-size:11px;max-width:230px}
    .tb-select:hover{border-color:var(--acc)}
    .tb-select:focus{outline:1px solid var(--acc)}
    #tb-menu{display:flex;align-items:center;gap:2px;flex:none}
    #tb-menu[hidden]{display:none}
    .tb-menu-item{background:none;border:none;color:var(--tx);font:inherit;cursor:pointer;
      padding:2px 10px;border-radius:4px}
    .tb-menu-item:hover,.tb-menu-item.open{background:#22303f}
    .tb-pop{position:fixed;top:40px;background:#161b22;border:1px solid var(--bd);
      border-radius:6px;padding:4px;z-index:60;box-shadow:0 10px 28px #0009;min-width:180px}
    .tb-pop[hidden]{display:none}
    .tb-pop button{display:block;width:100%;text-align:left;background:none;border:none;
      color:var(--tx);font:inherit;padding:6px 12px;border-radius:4px;cursor:pointer;
      white-space:nowrap}
    .tb-pop button:hover{background:#22303f}
    .tb-pop .sep{height:1px;background:var(--bd);margin:4px 6px}
    .tb-status{color:var(--dim);font-size:11px;white-space:nowrap;overflow:hidden;
      text-overflow:ellipsis;max-width:34vw;margin-left:auto}
    .tb-status.err{color:#f0883e}
    #tb-help{position:fixed;inset:0;z-index:70;display:grid;place-items:center;
      background:#0b0d11cc}
    #tb-help[hidden]{display:none}
    #tb-help .card{background:#161b22;border:1px solid var(--bd);border-radius:8px;
      padding:20px 24px;max-width:520px;line-height:1.8}
    #tb-help h3{margin:0 0 8px;color:var(--acc);font-size:14px}
    #tb-help kbd{background:#232b36;border:1px solid var(--bd);border-radius:3px;
      padding:0 4px;font:inherit;font-size:11px}
    #tb-help .close{float:right;cursor:pointer;color:var(--dim)}
  `;
  document.head.appendChild(style);

  // ── 重建标题栏 ──────────────────────────────────────────────────
  const top = document.getElementById("top");
  const entriesBox = document.getElementById("entries");
  const trail = document.getElementById("trail");

  const brand = document.createElement("span");
  brand.className = "tb-brand";
  brand.innerHTML =
    '<svg width="15" height="15" viewBox="0 0 32 32" aria-hidden="true">' +
    '<path d="M10 6V26M10 16h12" stroke="#6fb2ff" stroke-width="3" fill="none"/>' +
    '<circle cx="10" cy="6" r="3.5" fill="#6fb2ff"/>' +
    '<circle cx="10" cy="16" r="3.5" fill="#6fb2ff"/>' +
    '<circle cx="10" cy="26" r="3.5" fill="#6fb2ff"/>' +
    '<circle cx="22" cy="16" r="3" fill="none" stroke="#7c8595" stroke-width="2.5"/>' +
    "</svg><span>Clew</span>";

  const burger = document.createElement("button");
  burger.className = "tb-icon";
  burger.id = "tb-burger";
  burger.textContent = "☰";
  burger.title = "菜单";

  const projectSelect = document.createElement("select");
  projectSelect.className = "tb-select";
  projectSelect.id = "project-select";
  projectSelect.title = "切换项目";

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
  row1.appendChild(brand);
  row1.appendChild(burger);
  row1.appendChild(projectSelect);
  row1.appendChild(menu);

  const row2 = document.createElement("div");
  row2.id = "tb-row2";
  row2.appendChild(entryLabel);
  row2.appendChild(entriesBox);
  const sep = document.createElement("span");
  sep.textContent = "|";
  sep.style.color = "var(--dim)";
  row2.appendChild(sep);
  row2.appendChild(trail);
  row2.appendChild(status);

  top.innerHTML = "";
  top.appendChild(row1);
  top.appendChild(row2);

  // ── 弹出菜单 ────────────────────────────────────────────────────
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
        const sep = document.createElement("div");
        sep.className = "sep";
        pop.appendChild(sep);
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
    pop.hidden = false;
  }

  // ── 菜单栏模式：点开后 ☰ 自身也收起，只留 logo 和菜单项 ────────
  let menuMode = false;
  function setMenuMode(on) {
    menuMode = on;
    menu.hidden = !on;
    burger.style.display = on ? "none" : "";
    projectSelect.style.display = on ? "none" : "";
    if (!on) closePop();
  }

  burger.onclick = function (e) {
    e.stopPropagation();
    setMenuMode(!menuMode);
  };

  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") {
      if (!pop.hidden) closePop();
      else if (menuMode) setMenuMode(false);
    }
  });

  // 点到菜单以外的任何地方（画布、minimap、另一行……）就收起
  document.addEventListener("mousedown", function (e) {
    if (!pop.hidden && !pop.contains(e.target)) closePop();
    if (menuMode && !row1.contains(e.target) && !pop.contains(e.target)) {
      setMenuMode(false);
    }
  });

  // ── 文件 / 帮助 ─────────────────────────────────────────────────
  const help = document.createElement("div");
  help.id = "tb-help";
  help.hidden = true;
  help.innerHTML =
    '<div class="card"><span class="close">✕</span>' +
    "<h3>Clew</h3>" +
    "<p>从入口开始读调用图。点 <kbd>◆</kbd> 进入实现，类卡片按需长出方法目录。</p>" +
    "<p>空白处按住拖动画布，右下角 minimap 点哪儿跳哪儿。</p>" +
    "<p>顶栏下拉换入口。</p></div>";
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
      popAt(item, [{ label: "打开项目…", run: chooseFolder }]);
    } else {
      closePop();
      help.hidden = false;
    }
  });

  // ── 项目 ────────────────────────────────────────────────────────
  function recent() {
    try {
      return JSON.parse(localStorage.getItem(RECENT_KEY) || "[]");
    } catch {
      return [];
    }
  }

  function remember(path) {
    const list = recent().filter((p) => p.path !== path);
    list.unshift({ path: path, name: path.split(/[\\/]/).filter(Boolean).pop() || path });
    localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 10)));
  }

  function renderProjects(currentPath) {
    const list = recent();
    projectSelect.innerHTML = "";
    if (!list.length) {
      const o = document.createElement("option");
      o.textContent = "（还没有项目）";
      o.value = "";
      projectSelect.appendChild(o);
      projectSelect.disabled = true;
      return;
    }
    projectSelect.disabled = false;
    for (const p of list) {
      const o = document.createElement("option");
      o.value = p.path;
      o.textContent = p.name;
      o.title = p.path;
      if (p.path === currentPath) o.selected = true;
      projectSelect.appendChild(o);
    }
  }

  projectSelect.onchange = function () {
    if (projectSelect.value) openPath(projectSelect.value);
  };

  function setStatus(text, isError) {
    status.textContent = text || "";
    status.classList.toggle("err", !!isError);
  }

  async function openPath(path) {
    setStatus("正在打开（首次会先建索引）…");
    try {
      await api.invoke("open_project", { path: path });
      const data = await api.invoke("graph");
      bootProject(data);
      remember(path);
      renderProjects(path);
      setStatus("");
    } catch (err) {
      setStatus(String(err), true);
    }
  }

  async function chooseFolder() {
    let dir = null;
    try {
      dir = await window.__TAURI__.dialog.open({
        directory: true,
        multiple: false,
        title: "选择要分析的 Python 项目",
      });
    } catch (err) {
      setStatus(String(err), true);
      return;
    }
    if (!dir) return;
    await openPath(dir);
  }

  // ── 启动 ────────────────────────────────────────────────────────
  const startPath = window.__CLEW_PATH__ || null;
  if (startPath) remember(startPath);
  renderProjects(startPath);

  window.clewShell = {
    openPath: openPath,
    chooseFolder: chooseFolder,
    setStatus: setStatus,
    setMenuMode: setMenuMode,
  };
})();
