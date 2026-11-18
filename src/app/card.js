// app/card.js
import { h } from "../shared/dom.js";
import { esc } from "../shared/format.js";
import { STAR_SVG, icon } from "../shared/icons.js";
import { SUMMARY } from "./env.js";
import { hasMore, nextState, toggleLabel } from "./format.js";
import { card, cardRec, placedEl, store } from "./store.js";
import { renderLine } from "./highlight.js";
import { closeCard, expand } from "./place.js";
import { relayout } from "./layout.js";
import { openFocus, refreshSelectionUI, select } from "./select.js";

// ─── 卡片种类 ────────────────────────────────────────────────────
// 差别只在两处：标题栏的徽标、正文怎么画。放在一张表里，加一种卡片就加一项。
const kinds = {
  class: {
    badges: function (n) {
      return `
        <span class="kindbadge">类</span>
        ${n.bases && n.bases.length
          ? `<span class="kindbadge">← ${esc(n.bases.join(", "))}</span>`
          : ""}`;
    },
    body: function (id, host) { renderAccordion(id, host); },
  },
  symbol: {
    badges: function () { return ""; },
    body: function (id, host, st) {
      const n = store.NODES[id];
      const shown = n.lines.length;
      const limit = st === "summary" ? SUMMARY : shown;
      n.lines.slice(0, limit).forEach(function (ln) { renderLine(ln, host, id); });
      if (st !== "summary" || !hasMore(id)) return;
      const rec = cardRec(id);
      const more = h("div", { class: "ln more", text: "还有 " + (shown - SUMMARY) + " 行…" });
      more.onclick = function (e) {
        e.stopPropagation();
        rec.state = "full";
        renderCard(id);
        relayout();
      };
      host.appendChild(more);
    },
  },
};

function kindOf(n) {
  return n.kind === "class" ? kinds.class : kinds.symbol;
}

export function renderCard(id) {
  const n = store.NODES[id];
  const rec = card(id);
  const el = rec.el;
  const st = rec.state || "summary";
  const k = kindOf(n);
  el.className = "card" + (st === "mini" ? " mini" : "");
  el.innerHTML = "";

  const head = h("div", { class: "head" });
  const pinned = window.__clewIsFavorite && window.__clewIsFavorite(id);
  head.innerHTML = `
    <span class="t">${esc(n.title)}</span>
    ${store.FANIN[id] > 1 ? `<span class="hub">被调 ${store.FANIN[id]}</span>` : ""}
    ${k.badges(n)}
    <span class="m">${esc(n.module)}</span>
    <span class="btn tg">${toggleLabel(id, st)}</span>
    <span class="btn pin${pinned ? " on" : ""}" title="标记为入口">${STAR_SVG}</span>
    ${store.roots.has(id) ? "" : `<span class="btn close">${icon("close")}</span>`}`;
  head.onclick = function () { select(id); };
  head.ondblclick = function (e) { e.stopPropagation(); openFocus(id); };
  head.querySelector(".tg").onclick = function (e) {
    e.stopPropagation();
    rec.state = nextState(id, st);
    renderCard(id);
    relayout();
  };
  const closeBtn = head.querySelector(".close");
  if (closeBtn) closeBtn.onclick = function (e) {
    e.stopPropagation();
    closeCard(id);
  };
  const pinBtn = head.querySelector(".pin");
  if (pinBtn) pinBtn.onclick = function (e) {
    e.stopPropagation();
    if (window.__clewToggleFavorite) window.__clewToggleFavorite(id);
  };
  el.appendChild(head);

  if (st !== "mini") {
    const body = h("div", { class: "body" });
    k.body(id, body, st);
    el.appendChild(body);
  }
  refreshSelectionUI(store.selectedId);
}

// ─── 类卡片：手风琴。行按需生成（被调用到才出现），可回收 ────────
export function methodSetOf(classId) {
  const rec = cardRec(classId);
  if (!rec.members) rec.members = new Set();
  return rec.members;
}

export function renderAccordion(classId, host) {
  const n = store.NODES[classId];
  // 构造行：只有这个类被实例化过才画；__init__ 沿 MRO 找，可能继承自父类
  const instantiated = (store.FANIN[classId] || 0) > 0;
  const ctorId = instantiated && n.ctor && store.NODES[n.ctor] ? n.ctor : null;
  if (ctorId) {
    const made = !!placedEl(ctorId);
    const inherited = n.ctor_owner && n.ctor_owner !== n.qualname;
    const box = h("div", { class: "acc" });
    const head = h("div", { class: "acc-head", dataset: { method: ctorId } });
    head.innerHTML = `
      <span class="caret">${icon("enter")}</span>
      <span class="nm">${esc(store.NODES[ctorId].title)}</span>
      <span class="cnt">${inherited ? `继承自 ${esc(n.ctor_owner)}` : "构造"}</span>`;
    head.title = made ? "定位到它的卡片" : "展开为卡片";
    head.onclick = function (e) {
      e.stopPropagation();
      if (placedEl(ctorId)) { select(ctorId); relayout(); }
      else expand(ctorId, classId);
    };
    box.appendChild(head);
    host.appendChild(box);
  } else if (instantiated && n.external_base) {
    // 构造来自项目外的基类：显示父类的 __init__ 名字，但不可展开
    const box = h("div", { class: "acc" });
    const head = h("div", { class: "acc-head" });
    // 不可展开的一行：地球＝构造来自项目外，和上面那行的 ↳ 形成"能进去 / 进不去"的对比
    head.innerHTML = `
      <span class="caret">${icon("external")}</span>
      <span class="nm">${esc(n.external_base)}.__init__(…)</span>
      <span class="cnt">构造 · 项目外</span>`;
    head.title = n.external_base +
      " 是内置或第三方类型，Clew 索引不到它的实现";
    head.style.opacity = ".72";
    box.appendChild(head);
    host.appendChild(box);
  }
  // 方法目录：只列用户实际进入过的（点行尾标记或点目录行产生）
  const set = methodSetOf(classId);
  if (set.size) {
    Array.from(set).sort().forEach(function (mid) {
      if (isCtor(mid)) return;           // 构造行是固定的，不重复列
      const m = store.NODES[mid];
      if (!m) return;
      const made = !!placedEl(mid);
      const box = h("div", { class: "acc" });
      const head = h("div", { class: "acc-head", dataset: { method: mid } });
      head.innerHTML = `
        <span class="caret">${icon("enter")}</span>
        <span class="nm">${esc(m.qualname.split(".").pop())}(${m.params.join(", ")})</span>
        <span class="cnt">被调 ${store.FANIN[mid] || 0}</span>`;
      head.title = made ? "定位到它的卡片" : "展开为卡片";
      head.onclick = function (e) {
        e.stopPropagation();
        expand(mid, classId);
      };
      box.appendChild(head);
      host.appendChild(box);
    });
  } else {
    const note = h("div", { class: "hidden-note", text: "还没有走过本类的方法（目录只收录走过的，点行尾标记指进来才补）" });
    host.appendChild(note);
  }
}

// 方法不是卡片：展开 = 让所属类出现 + 该行出现 + 展开
// 判据是卡片种类，不是名字里有没有点：嵌套函数也带点，但它是一张普通卡片
export function isMethod(id) {
  return store.NODES[id] && store.NODES[id].kind === "method";
}
export function isCtor(id) {
  const n = store.NODES[id];
  return !!n && n.qualname.split(".").pop() === "__init__";
}

// 只去掉最后一段，嵌套类的方法才归到最近的那个类
export function classOf(id) {
  const n = store.NODES[id];
  const dot = n.qualname.lastIndexOf(".");
  return n.module + ":" + (dot > 0 ? n.qualname.slice(0, dot) : n.qualname);
}
