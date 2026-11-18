// shell/sidetab.js
import { store } from "./store.js";
import { ICON } from "./util.js";
import { hideWelcome, showWelcome } from "./confirm.js";
import { setMenuMode } from "./filemenu.js";
import { chooseFolder, closeProject, openPath } from "./project.js";
import { setStatus } from "./titlebar.js";
import { favorites, toggleFavorite } from "./favorites.js";
import { rail } from "./sidebar.js";
import { spList, spListSearch, spPaneSearch, spPaneTree } from "./sidebase.js";
import { spSearch, spSearchPage, spSearchRender } from "./sidesearch.js";
import { setView, syncViewLabel } from "./viewmenu.js";
import { collectRows } from "./dirtree.js";
import { spRender, toggleDir } from "./siderows.js";
import { setSideOpen, spLoad, spReset, toggleFile, toggleSym } from "./sidetoggle.js";

// ── 活动栏 tab ──────────────────────────────────────────────────
export let sideTab = "tree";

export function applyTab() {
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
  if (store.sideOpen && tab === sideTab) {
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
  if (store.spView === "fav") spRender();
  // 卡片头那颗星
  if (window.__clewRefreshCard) window.__clewRefreshCard(id);
});

export async function onTreeClick(e) {
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
export const rootChangedBefore = window.__clewRootChanged;
window.__clewRootChanged = function (rootId) {
  if (rootChangedBefore) rootChangedBefore(rootId);
  if (!store.sideOpen) return;
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
export function spSyncProject() {
  if (!store.currentPath) {
    store.spFiles = null;
    store.spLoadedFor = null;
    spReset();
    setSideOpen(false);
    return;
  }
  if (!store.sideOpen) return; // 收着的侧栏不该在后台白取一份数据
  spLoad();
}
window.__clewSide = {
  toggle: setSideOpen,
  reload: function () {
    store.spLoadedFor = null;
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
if (store.currentPath) {
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

