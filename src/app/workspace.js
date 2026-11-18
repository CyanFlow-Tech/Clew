// app/workspace.js
import { ensure } from "./data.js";
import { graph } from "./dom.js";
import { card, placedEl, placedIds, store } from "./store.js";
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
    // 卡片状态照存，DOM 不存：快照只记「哪些卡、展开到什么程度、父卡是谁」
    const cards = {};
    Object.keys(t.cards || {}).forEach(function (id) {
      const c = t.cards[id];
      cards[id] = {
        state: c.state,
        parent: c.parent,
        members: c.members ? Array.from(c.members) : null,
      };
    });
    list.push({
      root: rootId,
      cards: cards,
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

/// 合并之前写下的快照把状态摊在三张平行表里，这里折回一条记录
function cardsFromOldShape(s) {
  const out = {};
  Object.keys(s.state || {}).forEach(function (id) {
    out[id] = { state: s.state[id], parent: (s.parentOf || {})[id] || null };
  });
  Object.keys(s.methodSet || {}).forEach(function (id) {
    if (!out[id]) out[id] = { state: "summary", parent: null };
    out[id].members = s.methodSet[id];
  });
  return out;
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
    const saved = keepAlive(s.cards || cardsFromOldShape(s), alive);
    t.cards = {};
    Object.keys(saved).forEach(function (id) {
      const c = saved[id] || {};
      t.cards[id] = {
        el: null,
        state: c.state || "summary",
        parent: c.parent || null,
        members: c.members ? new Set(c.members) : null,
      };
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
  Object.keys(store.cards).forEach(function (id) {
    if (store.NODES[id]) placeCard(id, card(id) ? card(id).parent : null);
  });
  placedIds().forEach(function (id) { renderCard(id); });
}

export function syncActiveTab() {
  if (!store.activeRoot) return;
  const t = store.tabs.get(store.activeRoot);
  if (!t) return;
  t.cards = store.cards;
  t.roots = store.roots;
  t.activated = store.activated;
  t.selectedId = store.selectedId;
  t.selectedRow = store.selectedRow;
  t.trail = store.trail;
  t.scroll = { left: graph.scrollLeft, top: graph.scrollTop };

}

export function saveTab() {
  syncActiveTab();
  persistWorkspace();
}

// 把一张卡片带回视口左上角。画布是滚动容器，卡片坐标是 world 里的绝对坐标。
export function scrollToCard(id) {
  const el = placedEl(id);
  if (!el) return;
  const x = parseFloat(el.style.left) || 0;
  const y = parseFloat(el.style.top) || 0;
  graph.scrollLeft = Math.max(0, x - 40);
  graph.scrollTop = Math.max(0, y - 30);
}


