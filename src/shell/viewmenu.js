// shell/viewmenu.js
import { store } from "./store.js";
import { api } from "./boot.js";
import { SP_VIEWS, SP_VIEW_KEY } from "./sidebar.js";
import { spViewBtn, spViewLabel } from "./sidebase.js";
import { spRender } from "./siderows.js";

export const spMenu = document.createElement("div");
spMenu.className = "tb-pop";
spMenu.id = "sp-menu";
spMenu.hidden = true;
document.body.appendChild(spMenu);


export function viewLabel(id) {
  const v = SP_VIEWS.filter((x) => x.id === id)[0];
  return v ? v.label : SP_VIEWS[0].label;
}

export function syncViewLabel() {
  spViewLabel.textContent = viewLabel(store.spView);
}

export function closeSpMenu() {
  spMenu.hidden = true;
  spViewBtn.classList.remove("open");
}

export function openSpMenu() {
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

export function setView(id) {
  store.spView = id;
  try {
    localStorage.setItem(SP_VIEW_KEY, id);
  } catch (err) {
    /* 隐私模式写不进就算了 */
  }
  syncViewLabel();
  spRender();
}

// 文件节点、类节点都是就地展开，所以节点里要能查到自己的定义
export async function ensureSyms(module) {
  if (store.spCache.has(module)) return store.spCache.get(module);
  try {
    const rows = await api.invoke("symbols_in", { module: module });
    store.spCache.set(module, rows || []);
  } catch (err) {
    if (window.__clewLog) window.__clewLog("symbols_in " + module + " 失败：" + err);
    store.spCache.set(module, []);
  }
  return store.spCache.get(module);
}

export async function ensureEntry() {
  if (store.spEntrySyms) return store.spEntrySyms;
  try {
    store.spEntrySyms = (await api.invoke("entry_symbols")) || [];
  } catch (err) {
    store.spEntrySyms = [];
  }
  return store.spEntrySyms;
}

// 入口视图也默认收起目录：先只看到顶层目录，点哪层看哪层。
// （之前是切过来就把所有目录摊开，行数一多反而没法扫。）

// 一个文件里的定义按点号嵌套：Class.method 挂在 Class 底下
