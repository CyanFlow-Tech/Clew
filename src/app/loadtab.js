// app/loadtab.js
import { ensure } from "./data.js";
import { graph, svg, world } from "./dom.js";
import { placedEl, placedIds, store } from "./store.js";
import { renderCard } from "./card.js";
import { expand } from "./place.js";
import { relayout } from "./layout.js";
import { draw, hideBubble } from "./edge.js";
import { closeFocus, ensureTab, refreshSelectionUI, renderTrail } from "./select.js";
import { hydrateTab, persistWorkspace, readWorkspace, restoreWorkspace, saveTab, scrollToCard } from "./workspace.js";

export function loadTab(rootId) {
  const t = ensureTab(rootId);
  if (!t) return;
  store.activeRoot = rootId;
  store.cards = t.cards;
  store.roots = t.roots;
  store.activated = t.activated;
  store.selectedId = t.selectedId;
  store.selectedRow = t.selectedRow;
  store.trail = t.trail;

  hideBubble();
  closeFocus();

  // 换掉画布里的卡片，svg 留着当连线层
  Array.prototype.slice.call(world.children).forEach(function (el) {
    if (el !== svg) world.removeChild(el);
  });
  placedIds(t).forEach(function (id) { world.appendChild(t.cards[id].el); });
  hydrateTab();

  refreshSelectionUI(store.selectedId);
  renderTrail();
  relayout();
  // 画布滚动量是全局的，不跟着 tab 走。换根时把上一个根滚到的位置留给它，
  // 新根回到左上角，否则新根的卡片落在视口外面，看着像一张都没画出来。
  graph.scrollLeft = t.scroll.left;
  graph.scrollTop = t.scroll.top;
  if (window.__clewRootChanged) window.__clewRootChanged(rootId);
}

// 从入口面板打开任意一个符号作根。打开过就复用它的 tab。
export async function openRoot(rootId) {
  if (!rootId || rootId === store.activeRoot) {
    if (rootId && window.__clewRootOpened) window.__clewRootOpened(rootId);
    // 已经在这个根上了。点了不该像没反应，至少把它的卡片带回眼前
    scrollToCard(rootId);
    return;
  }
  const t = ensureTab(rootId);
  if (!t) return;
  // 先把根的卡片取回来再动画布。反过来的话，任何一次取数失败都会把画布清空，
  // 之后点哪个入口都只剩空白，只能靠重启解救。
  if (!placedIds(t).length && !store.NODES[rootId]) {
    try {
      await ensure([rootId]);
    } catch (err) {
      if (window.__clewLog) window.__clewLog("openRoot " + rootId + " 失败：" + err);
    }
    if (!store.NODES[rootId]) {
      if (window.__clewNote) {
        window.__clewNote("取不到 " + rootId + " 的卡片，索引可能不完整", true);
      }
      return;
    }
  }
  saveTab();
  loadTab(rootId);
  if (!placedIds(t).length) {
    // 只有第一次开这个根才去取它的卡片
    store.roots.add(rootId);
    await expand(rootId);
    saveTab();
  }
  if (window.__clewRootOpened) window.__clewRootOpened(rootId);
}

// 外壳换项目时调这一个：画布的数据、画布、每个根的状态全部重建
window.bootProject = bootProject;
window.openRoot = openRoot;
window.getActiveRoot = function () { return store.activeRoot; };
window.__clewRefreshCard = function (id) {
  if (placedEl(id)) renderCard(id);
};

// 换项目时整份换掉：数据、画布、每个根的状态全部重建
export async function bootProject(entriesOrRoot) {
  // 兼容旧调用：传入口数组就取第一个作根，传字符串就是根本身，传空数组表示清空
  if (Array.isArray(entriesOrRoot)) store.ENTRIES = entriesOrRoot || [];
  // 按需取：换项目从空图开始，点开谁取谁
  store.NODES = {};
  store.CALLS = {};

  hideBubble();
  closeFocus();
  Array.prototype.slice.call(world.children).forEach(function (el) {
    if (el !== svg) world.removeChild(el);
  });

  store.cards = {};
  store.roots = new Set();
  store.activated = new Set();
  store.selectedId = null;
  store.selectedRow = null;
  store.trail = [];
  store.tabs.clear();
  store.activeRoot = null;

  // 打开项目时先看有没有上次的工作区：有就按它重建，别再回默认入口
  const wantsRestore = Array.isArray(entriesOrRoot)
    ? entriesOrRoot.length > 0
    : typeof entriesOrRoot === "string" && !!entriesOrRoot;
  if (wantsRestore) {
    try {
      const snap = readWorkspace();
      if (snap && (await restoreWorkspace(snap))) {
        if (window.__clewRootOpened) window.__clewRootOpened(store.activeRoot);
        return;
      }
    } catch (err) {
      if (window.__clewLog) window.__clewLog("恢复工作区失败：" + err);
    }
  }

  const rootId = Array.isArray(entriesOrRoot)
    ? (store.ENTRIES.find(function (e) { return e && e.id; }) || {}).id
    : entriesOrRoot;

  if (rootId) {
    loadTab(rootId);
    store.roots.add(rootId);
    await expand(rootId);
    saveTab();
    if (window.__clewRootOpened) window.__clewRootOpened(rootId);
  } else if (window.__clewRootChanged) {
    window.__clewRootChanged(null);
  }

  if (window.__CLEW_MARK) {
    window.__CLEW_MARK("first-card");
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        window.__CLEW_MARK("paint");
        if (window.__CLEW_REPORT) window.__CLEW_REPORT();
      });
    });
  }
}

bootProject(store.ENTRIES).catch(function (err) {
  console.error("启动失败", err);
});

window.addEventListener("resize", function () { relayout(); });
graph.addEventListener("scroll", draw);
graph.addEventListener("scroll", function () { persistWorkspace(); });

// 只有用户真的在卡片上点过东西，才算「用过」这张卡：光打开不算。
// 用捕获阶段挂在画布上，卡片内部的任何点击都能收到，不用逐个绑。
(function noteCardUse() {
  let lastId = null;
  let lastTs = 0;
  graph.addEventListener(
    "click",
    function (e) {
      const card = e.target && e.target.closest ? e.target.closest(".card") : null;
      if (!card || !card.dataset.id) return;
      const id = card.dataset.id;
      const now = Date.now();
      // 同一张卡连点几下只记一次，避免把最近列表刷满同一项
      if (id === lastId && now - lastTs < 5000) return;
      lastId = id;
      lastTs = now;
      if (window.__clewCardUsed) window.__clewCardUsed(id);
    },
    true
  );
})();



