// app/workspace.js
import { ensure } from "./data.js";
import { graph, svg, world } from "./dom.js";
import { store } from "./store.js";
import { renderCard } from "./card.js";
import { placeCard } from "./place.js";
import { blankTab } from "./select.js";
import { loadTab } from "./loadtab.js";

// ─── 工作区快照 ──────────────────────────────────────────────────
// 卡片本身是 DOM，不能直接存；存的是「哪些卡、各自展开到什么程度、选了什么、
// 滚到哪」，重启后按这份数据重建。存在 localStorage，按项目路径分开。
export const WS_PREFIX = "clew.workspace.";

export function workspaceKey() {
  const p = window.__CLEW_PROJECT__;
  return p ? WS_PREFIX + p : null;
}

export function snapshotWorkspace() {
  if (!store.activeRoot || !store.tabs.size) return null;
  const list = [];
  store.tabs.forEach(function (t, rootId) {
    if (!t || !rootId) return;
    const methodSets = {};
    Object.keys(t.methodSet || {}).forEach(function (k) {
      methodSets[k] = Array.from(t.methodSet[k] || []);
    });
    list.push({
      root: rootId,
      state: t.state || {},
      openMethods: t.openMethods || {},
      parentOf: t.parentOf || {},
      methodSet: methodSets,
      roots: Array.from(t.roots || []),
      activated: Array.from(t.activated || []),
      selectedId: t.selectedId || null,
      selectedRow: t.selectedRow || null,
      trail: t.trail || [],
      scroll: t.scroll || { left: 0, top: 0 },
    });
  });
  return { activeRoot: store.activeRoot, tabs: list };
}

export let wsTimer = null;
export function persistWorkspace() {
  const key = workspaceKey();
  if (!key) return;
  syncActiveTab(); // 先让 store.tabs 里的当前 tab 是最新的
  clearTimeout(wsTimer);
  wsTimer = setTimeout(function () {
    try {
      const snap = snapshotWorkspace();
      if (snap) localStorage.setItem(key, JSON.stringify(snap));
    } catch (err) {
      // 配额满了或者隐私模式：丢一份快照不影响用，别打断操作
    }
  }, 400);
}

export function readWorkspace() {
  const key = workspaceKey();
  if (!key) return null;
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const snap = JSON.parse(raw);
    return snap && snap.tabs && snap.tabs.length ? snap : null;
  } catch (err) {
    return null;
  }
}

/// 只保留索引里还存在的 id：项目改过之后旧快照可能指向不存在的卡
export function keepAlive(obj, alive) {
  const out = {};
  Object.keys(obj || {}).forEach(function (k) {
    if (alive[k]) out[k] = obj[k];
  });
  return out;
}

export async function restoreWorkspace(snap) {
  const ids = [];
  snap.tabs.forEach(function (s) {
    Object.keys(s.state || {}).forEach(function (id) {
      if (ids.indexOf(id) < 0) ids.push(id);
    });
    (s.roots || []).forEach(function (id) {
      if (ids.indexOf(id) < 0) ids.push(id);
    });
  });
  if (!ids.length) return false;
  // 按需模式先一次性把卡片取回来，取不到的卡就当它没了
  await ensure(ids);
  const alive = {};
  ids.forEach(function (id) {
    if (store.NODES[id]) alive[id] = true;
  });
  if (!Object.keys(alive).length) return false;

  snap.tabs.forEach(function (s) {
    if (!alive[s.root]) return;
    const t = blankTab(s.root);
    t.state = keepAlive(s.state, alive);
    t.openMethods = keepAlive(s.openMethods, alive);
    t.parentOf = keepAlive(s.parentOf, alive);
    t.methodSet = {};
    Object.keys(s.methodSet || {}).forEach(function (k) {
      if (!alive[k]) return;
      t.methodSet[k] = new Set(s.methodSet[k] || []);
    });
    t.roots = new Set((s.roots || []).filter(function (id) { return alive[id]; }));
    // activated 里是 call site 的 id，不是卡片 id：用卡片表过滤会把线全丢掉
    t.activated = new Set((s.activated || []).filter(function (id) { return !!store.CALLS[id]; }));
    t.selectedId = s.selectedId && alive[s.selectedId] ? s.selectedId : null;
    t.selectedRow = s.selectedRow && alive[s.selectedRow] ? s.selectedRow : null;
    t.trail = (s.trail || []).filter(function (id) { return alive[id]; });
    t.scroll = s.scroll || { left: 0, top: 0 };
    t.needsHydrate = true;
    store.tabs.set(s.root, t);
  });
  if (!store.tabs.size) return false;

  const active = snap.activeRoot && store.tabs.has(snap.activeRoot) ? snap.activeRoot : snap.tabs[0].root;
  loadTab(active);
  return true;
}

/// 恢复出来的 tab 只有数据没有 DOM，切到它时才按 state 把卡片建出来
export function hydrateTab() {
  const t = store.tabs.get(store.activeRoot);
  if (!t || !t.needsHydrate) return;
  t.needsHydrate = false;
  Object.keys(store.state).forEach(function (id) {
    if (store.NODES[id]) placeCard(id, store.parentOf[id]);
  });
  Object.keys(store.cardEls).forEach(function (id) {
    renderCard(id);
  });
}

export function syncActiveTab() {
  if (!store.activeRoot) return;
  const t = store.tabs.get(store.activeRoot);
  if (!t) return;
  t.cardEls = store.cardEls;
  t.state = store.state;
  t.openMethods = store.openMethods;
  t.parentOf = store.parentOf;
  t.methodSet = store.methodSet;
  t.roots = store.roots;
  t.activated = store.activated;
  t.selectedId = store.selectedId;
  t.selectedRow = store.selectedRow;
  t.trail = store.trail;
  t.scroll = { left: graph.scrollLeft, top: graph.scrollTop };
  t.nodes = Array.prototype.slice
    .call(world.children)
    .filter(function (el) { return el !== svg; });
}

export function saveTab() {
  syncActiveTab();
  persistWorkspace();
}

// 把一张卡片带回视口左上角。画布是滚动容器，卡片坐标是 world 里的绝对坐标。
export function scrollToCard(id) {
  const el = store.cardEls[id];
  if (!el) return;
  const x = parseFloat(el.style.left) || 0;
  const y = parseFloat(el.style.top) || 0;
  graph.scrollLeft = Math.max(0, x - 40);
  graph.scrollTop = Math.max(0, y - 30);
}
