// app/select.js
import { h } from "../shared/dom.js";
import { showId } from "../shared/format.js";
import { focusLayer, world } from "./dom.js";
import { card, placedEl, placedIds, store } from "./store.js";
import { classOf, isMethod } from "./card.js";
import { expand } from "./place.js";

// ─── 聚焦 / 选中 / 路径条 ───────────────────────────────────────
export function openFocus(id) {
  focusLayer.style.display = "block";
  focusLayer.innerHTML = "";
  const clone = placedEl(id).cloneNode(true);
  clone.style.position = "relative";
  clone.style.left = "auto";
  clone.style.top = "auto";
  clone.querySelectorAll(".anchor, .btn").forEach(function (a) { a.remove(); });
  focusLayer.appendChild(clone);
}
export function closeFocus() { focusLayer.style.display = "none"; focusLayer.innerHTML = ""; }
document.addEventListener("keydown", function (e) { if (e.key === "Escape") closeFocus(); });
focusLayer.addEventListener("click", function (e) { if (e.target === focusLayer) closeFocus(); });

export function chainOf(id) {
  const out = [], guard = new Set();
  let cur = id;
  while (cur && !guard.has(cur)) {
    guard.add(cur);
    out.unshift(cur);
    const rec = card(cur);
    cur = rec ? rec.parent : null;
  }
  return out;
}
export function select(id) {
  store.selectedId = id;
  store.selectedRow = isMethod(id) ? id : null;   // __init__ 也参与
  refreshSelectionUI(store.selectedId);
  store.trail = chainOf(id);
  renderTrail();
}

// 单一入口：按 selectedId / selectedRow 重算全部高亮，不重建 DOM
export function refreshSelectionUI(currentId) {
  placedIds().forEach(function (k) {
    const el = placedEl(k);
    if (!el || !el.classList) return;
    el.classList.toggle("sel", k === currentId);
    const owner = store.selectedRow ? classOf(store.selectedRow) : null;
    el.classList.toggle("related", !!owner && k === owner && k !== currentId);
  });
  const heads = world.querySelectorAll ? world.querySelectorAll(".acc-head[data-method]") : [];
  Array.prototype.forEach.call(heads, function (head) {
    head.classList.toggle("sel-row", store.selectedRow !== null && head.dataset.method === store.selectedRow);
  });
}
export function renderTrail() {
  const el = document.getElementById("trail");
  el.innerHTML = "";
  store.trail.forEach(function (id, i) {
    const s = h("span", { text: store.NODES[id] ? store.NODES[id].title : showId(id) });
    if (i === store.trail.length - 1) s.className = "cur";
    s.onclick = function () { expand(id); };
    el.appendChild(s);
    if (i < store.trail.length - 1) {
      const sep = h("i", { text: "›" });
      el.appendChild(sep);
    }
  });
}

// ─── 根：每个根一份独立的图 ─────────────────────────────────────

export function blankTab(rootId) {
  return {
    entryId: rootId,
    label: rootId,
    cards: {},
    roots: new Set(),
    activated: new Set(),
    selectedId: null,
    selectedRow: null,
    trail: [],
    nodes: [],
    // 每个根自己记住看过的位置，新根从左上角开始
    scroll: { left: 0, top: 0 },
  };
}

// 根只登记不预建：真正切到某个根时才建它的卡片
export function ensureTab(rootId) {
  if (!rootId) return null;
  if (!store.tabs.has(rootId)) store.tabs.set(rootId, blankTab(rootId));
  return store.tabs.get(rootId);
}

