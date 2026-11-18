// app/edge.js
import { h } from "../shared/dom.js";
import { esc, showQual } from "../shared/format.js";
import { bubble, svg, world } from "./dom.js";
import { placedEl, store } from "./store.js";
import { classOf, isMethod } from "./card.js";
import { expand } from "./place.js";

// ─── 连线 ───────────────────────────────────────────────────────

export function anchorOfCs(csId) {
  const nodes = world.querySelectorAll(".anchor");
  for (let i = 0; i < nodes.length; i++) {
    if (nodes[i].dataset && nodes[i].dataset.cs === csId) {
      const r = nodes[i].getBoundingClientRect(), w = world.getBoundingClientRect();
      return { x: r.left - w.left + r.width, y: r.top - w.top + r.height / 2 };
    }
  }
  return null;
}
export function targetAnchor(cid) {
  if (placedEl(cid)) return leftPort(cid);
  if (isMethod(cid)) {
    const cls = classOf(cid);
    if (placedEl(cls)) return leftPort(cls);
  }
  return null;
}

export function leftPort(id) {
  const el = placedEl(id);
  const r = el.getBoundingClientRect(), w = world.getBoundingClientRect();
  return { x: r.left - w.left, y: r.top - w.top + 15 };
}
export function rightPort(id) {
  const el = placedEl(id);
  const r = el.getBoundingClientRect(), w = world.getBoundingClientRect();
  return { x: r.right - w.left, y: r.top - w.top + 15 };
}
export function path(d, color, dash) {
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", d);
  p.setAttribute("stroke", color);
  p.setAttribute("stroke-width", "1.3");
  p.setAttribute("fill", "none");
  if (dash) p.setAttribute("stroke-dasharray", "4 3");
  svg.appendChild(p);
}
export function draw() {
  svg.innerHTML = "";
  const bus = {};
  Object.keys(store.CALLS).forEach(function (k) {
    const c = store.CALLS[k];
    if (!store.activated.has(k)) return;
    if (!c.to || !placedEl(c.from)) return;
    const b0 = targetAnchor(c.to);
    if (!b0) return;
    const a = anchorOfCs(k);
    const color = c.conf === "heur" ? "#d2a24c"
      : c.conf === "poly" ? "#bc8cff"
      : (c.from === c.to ? "#f0883e" : "#3fb950");
    const b = b0;
    if (!a) {
      // 行不在屏幕上（卡片折叠/方法收起）→ 从卡片右边缘合并出一条
      bus[c.from] = bus[c.from] || [];
      bus[c.from].push({ b: b, color: color });
      return;
    }
    const mx = (a.x + b.x) / 2;
    path("M " + a.x + " " + a.y + " C " + mx + " " + a.y + ", " + mx + " " + b.y + ", "
      + b.x + " " + b.y, color, c.from === c.to);
  });
  Object.keys(bus).forEach(function (id) {
    const a = rightPort(id), busX = a.x + 24;
    path("M " + a.x + " " + a.y + " L " + busX + " " + a.y, "#3a4652");
    bus[id].forEach(function (e) {
      const mx = Math.max(busX + 16, (busX + e.b.x) / 2);
      path("M " + busX + " " + a.y + " C " + mx + " " + a.y + ", " + mx + " " + e.b.y + ", "
        + e.b.x + " " + e.b.y, e.color);
    });
  });
}

// ─── 气泡 ───────────────────────────────────────────────────────
export let hoverCs = null;
export function argRows(cs) {
  const c = store.CALLS[cs];
  const params = store.NODES[c.to] ? store.NODES[c.to].params : [];
  if (!params.length) return '<div class="row"><span class="k">(无参数)</span><span class="v"></span></div>';
  return params.map(function (p) {
    const v = (c.args || {})[p];
    const value = v === undefined ? '<span style="color:#6b7280">未知</span>' : esc(v);
    return `<div class="row"><span class="k">${esc(p)}</span><span class="v">${value}</span></div>`;
  }).join("");
}
export function confNote(c) {
  if (c.conf === "heur") return "启发式匹配（按名字唯一命中，可能错）";
  if (c.conf === "poly") {
    return c.to
      ? "多态：静态只知道方法名，下面是全部同名实现"
      : "接收者类型未知，没挑目标，下面是全部同名实现";
  }
  return "精确匹配（同模块 / import / 类型推断 / self. 方法）";
}
export function showBubble(cs, ev) {
  hoverCs = cs;
  const c = store.CALLS[cs];
  bubble.innerHTML = "";
  const hdr = h("div", { class: "hdr", text: c.from + " → " + (c.to || c.target || "?") + (c.from === c.to ? " · 递归" : "") });
  bubble.appendChild(hdr);
  const args = document.createElement("div");
  args.innerHTML = argRows(cs);
  bubble.appendChild(args);
  if (c.candidates && c.candidates.length > 1) {
    const list = document.createElement("div");
    list.style.marginTop = "5px";
    c.candidates.forEach(function (cid, i) {
      const ref = store.NODES[cid] || store.REFS[cid];
      if (!ref) return;
      const row = document.createElement("div");
      row.style.cssText = "padding:2px 4px;border-radius:3px;cursor:pointer;color:var(--acc)";
      row.textContent = (i + 1) + ". " + ref.module + " · " + showQual(ref.qualname);
      row.onmouseenter = function () { row.style.background = "#2d333b"; };
      row.onmouseleave = function () { row.style.background = "transparent"; };
      row.onclick = function () { hideBubble(); expand(cid, c.from, cs); };
      list.appendChild(row);
    });
    bubble.appendChild(list);
  }
  const note = h("div", { class: "note" });
  note.textContent = (c.typed ? "类型推断：" + c.typed + " · " : "") +
    (c.imported ? "来源：" + c.imported + " · " : "") + confNote(c);
  bubble.appendChild(note);
  bubble.style.display = "block";
  bubble.style.left = Math.min(ev.clientX + 14, window.innerWidth - 480) + "px";
  bubble.style.top = Math.min(ev.clientY + 14, window.innerHeight - 40 - bubble.offsetHeight) + "px";
}
export function hideBubble() { hoverCs = null; bubble.style.display = "none"; }



