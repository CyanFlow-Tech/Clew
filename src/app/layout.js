// app/layout.js
import { CARD_W, COL_GAP, ROW_GAP } from "./env.js";
import { placedEl, placedIds, store } from "./store.js";
import { incomingPlaced } from "./place.js";
import { draw } from "./edge.js";

// ─── 布局：分层 + 同列按父位置排序（稳定顺序） ──────────────────
export function layoutOrder() {
  const placed = placedIds();
  const placedSet = new Set(placed);
  const depth = {};
  const rootsPlaced = placed.filter(function (id) {
    const inc = incomingPlaced(id);
    return inc.length === 0;
  });
  rootsPlaced.forEach(function (r) { depth[r] = 0; });

  const queue = rootsPlaced.slice();
  const seen = new Set(queue);
  while (queue.length) {
    const cur = queue.shift();
    Object.keys(store.CALLS).forEach(function (k) {
      const c = store.CALLS[k];
      if (c.from !== cur || !c.to || !placedSet.has(c.to)) return;
      const d = depth[cur] + 1;
      if (depth[c.to] === undefined || depth[c.to] < d) depth[c.to] = d;
      if (!seen.has(c.to)) { seen.add(c.to); queue.push(c.to); }
    });
  }
  placed.forEach(function (id) { if (depth[id] === undefined) depth[id] = 0; });

  const cols = {};
  placed.forEach(function (id) {
    const d = depth[id];
    (cols[d] = cols[d] || []).push(id);
  });
  Object.keys(cols).forEach(function (d) {
    cols[d].sort(function (a, b) {
      const pa = avgParentY(a), pb = avgParentY(b);
      if (pa !== pb) return pa - pb;
      return a < b ? -1 : a > b ? 1 : 0;
    });
  });
  return { cols: cols, depth: depth };
}

export function avgParentY(id) {
  const inc = incomingPlaced(id).filter(function (p) { return placedEl(p); });
  if (!inc.length) return 0;
  let sum = 0;
  inc.forEach(function (p) { sum += Number(placedEl(p).style.top.replace("px", "")) || 0; });
  return sum / inc.length;
}

// 重排之后要做别的事（小地图重画）的模块在这里挂一个回调，
// 而不是把这个函数本身改掉——导入进来的是常量绑定，改不了。
let afterLayout = null;

export function onRelayout(fn) {
  afterLayout = fn;
}

export function relayout() {
  const { cols } = layoutOrder();
  const depths = Object.keys(cols).map(Number).sort(function (a, b) { return a - b; });
  let x = 40;
  depths.forEach(function (d) {
    let y = 50;
    cols[d].forEach(function (id) {
      const el = placedEl(id);
      if (!el) return;
      el.style.left = x + "px";
      el.style.top = y + "px";
      y += el.offsetHeight + ROW_GAP;
    });
    const widest = Math.max.apply(null, cols[d].map(function (id) {
      return (placedEl(id) && placedEl(id).offsetWidth) || CARD_W;
    }));
    x += widest + COL_GAP;
  });
  animateDraw(280);
  if (afterLayout) afterLayout();
}

// 过渡动画期间持续重画，否则线会停在旧位置
export function animateDraw(ms) {
  const t0 = performance.now();
  (function step() {
    draw();
    if (performance.now() - t0 < ms) requestAnimationFrame(step);
  })();
}



