// app/place.js
import { ensure } from "./data.js";
import { world } from "./dom.js";
import { cardRec, placedEl, store } from "./store.js";
import { classOf, isCtor, isMethod, methodSetOf, renderCard } from "./card.js";
import { relayout } from "./layout.js";
import { hideBubble } from "./edge.js";
import { select } from "./select.js";
import { persistWorkspace } from "./workspace.js";

// ─── 放置 / 关闭 / 引用计数 ─────────────────────────────────────
export function placeCard(id, parentHint) {
  if (!store.NODES[id] || placedEl(id)) return;
  const el = document.createElement("div");
  el.dataset.id = id;
  el.style.transition = "none";
  world.appendChild(el);
  const rec = cardRec(id);
  rec.el = el;
  const parent = parentHint || firstPlacedCaller(id);
  if (parent) rec.parent = parent;
  renderCard(id);
  requestAnimationFrame(function () { el.style.transition = ""; });
}

export async function expand(id, parentHint, viaCs) {
  if (!id) return;
  if (!store.NODES[id]) {
    try {
      await ensure([id]);
    } catch (err) {
      if (window.__clewLog) window.__clewLog("expand " + id + " 失败：" + err);
    }
    if (!store.NODES[id]) {
      // 取不到就说明这一跳没得画，说出来，别静默什么都不发生
      if (window.__clewNote) {
        window.__clewNote("取不到 " + id + " 的卡片，索引可能不完整", true);
      }
      return;
    }
  }
  if (viaCs) store.activated.add(viaCs);

  // 方法落地时，它的类卡要同时在图上：类目录和函数卡片一起出现，
  // 选中才能做到方法卡、类卡、目录行三处联动高亮
  if (isMethod(id)) {
    const cls = classOf(id);
    if (!isCtor(id)) methodSetOf(cls).add(id);
    if (!placedEl(cls) && store.NODES[cls]) placeCard(cls, parentHint);
    if (placedEl(cls)) renderCard(cls);   // 目录里补上这一行
  }

  placeCard(id, isMethod(id) && placedEl(classOf(id)) ? classOf(id) : parentHint);

  // 类被实例化就意味着构造会跑：类目录和 __init__ 卡片一起出来
  const self = store.NODES[id];
  if (self && self.kind === "class" && (store.FANIN[id] || 0) > 0 && self.ctor && store.NODES[self.ctor]) {
    placeCard(self.ctor, id);
  }

  select(id);
  relayout();
  persistWorkspace();
}

export function firstPlacedCaller(id) {
  const k = Object.keys(store.CALLS).find(function (k) {
    return store.CALLS[k].to === id && placedEl(store.CALLS[k].from);
  });
  return k ? store.CALLS[k].from : null;
}

// 引用计数：谁还在引用这张卡
export function incomingPlaced(id) {
  return Object.keys(store.CALLS).filter(function (k) {
    return store.CALLS[k].to === id && placedEl(store.CALLS[k].from) && store.CALLS[k].from !== id;
  }).map(function (k) { return store.CALLS[k].from; });
}

// 关闭：先移掉被点的卡片，再按引用计数剪掉失去所有引用的后代
export function closeCard(id) {
  if (store.roots.has(id)) return;
  const children = Object.keys(store.CALLS)
    .filter(function (k) { return store.CALLS[k].from === id && store.CALLS[k].to; })
    .map(function (k) { return store.CALLS[k].to; });

  removeNode(id);

  children.forEach(function (child) {
    if (!placedEl(child) || store.roots.has(child)) return;
    if (incomingPlaced(child).length === 0) closeCard(child);
  });
  hideBubble();
  relayout();
}

export function removeNode(id) {
  const rec = store.cards[id];
  if (!rec || !rec.el) return;
  rec.el.remove();
  // 记录一起删：关掉的卡不该再被工作区快照恢复回来
  delete store.cards[id];
}


