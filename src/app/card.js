// app/card.js
import { esc } from "../shared/format.js";
import { STAR_SVG, icon } from "../shared/icons.js";
import { SUMMARY } from "./env.js";
import { hasMore, nextState, toggleLabel } from "./format.js";
import { store } from "./store.js";
import { renderLine } from "./highlight.js";
import { closeCard, expand } from "./place.js";
import { relayout } from "./layout.js";
import { openFocus, refreshSelectionUI, select } from "./select.js";

export function renderCard(id) {
  const n = store.NODES[id], st = store.state[id] || "summary", el = store.cardEls[id];
  el.className = "card" + (st === "mini" ? " mini" : "");
  el.innerHTML = "";

  const head = document.createElement("div");
  head.className = "head";
  head.innerHTML = '<span class="t">' + esc(n.title) + '</span>' +
    (store.FANIN[id] > 1 ? '<span class="hub">被调 ' + store.FANIN[id] + '</span>' : '') +
    (n.kind === "class" ? '<span class="kindbadge">类</span>' : '') +
    (n.kind === "class" && n.bases && n.bases.length ?
      '<span class="kindbadge">← ' + esc(n.bases.join(", ")) + '</span>' : '') +
    '<span class="m">' + esc(n.module) + '</span>' +
    '<span class="btn tg">' + toggleLabel(id, st) + '</span>' +
    '<span class="btn pin' +
      (window.__clewIsFavorite && window.__clewIsFavorite(id) ? " on" : "") +
      '" title="标记为入口">' + STAR_SVG + '</span>' +
    (store.roots.has(id) ? '' : '<span class="btn close">' + icon("close") + '</span>');
  head.onclick = function () { select(id); };
  head.ondblclick = function (e) { e.stopPropagation(); openFocus(id); };
  head.querySelector(".tg").onclick = function (e) {
    e.stopPropagation();
    store.state[id] = nextState(id, st);
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
    const body = document.createElement("div");
    body.className = "body";
    if (n.kind === "class") {
      renderAccordion(id, body);
      el.appendChild(body);
      refreshSelectionUI(store.selectedId);
      return;
    }
    const shown = n.lines.length;
    const limit = st === "summary" ? SUMMARY : shown;
    n.lines.slice(0, limit).forEach(function (ln) { renderLine(ln, body, id); });
    if (st === "summary" && hasMore(id)) {
      const more = document.createElement("div");
      more.className = "ln more";
      more.textContent = "还有 " + (shown - SUMMARY) + " 行…";
      more.onclick = function (e) { e.stopPropagation(); store.state[id] = "full"; renderCard(id); relayout(); };
      body.appendChild(more);
    }

    el.appendChild(body);
  }
  refreshSelectionUI(store.selectedId);
}

// ─── 类卡片：手风琴。行按需生成（被调用到才出现），可回收 ────────
export function methodSetOf(classId) {
  if (!store.methodSet[classId]) store.methodSet[classId] = new Set();
  return store.methodSet[classId];
}

export function renderAccordion(classId, host) {
  const card = store.NODES[classId];
  // 构造行：只有这个类被实例化过才画；__init__ 沿 MRO 找，可能继承自父类
  const instantiated = (store.FANIN[classId] || 0) > 0;
  const ctorId = instantiated && card.ctor && store.NODES[card.ctor] ? card.ctor : null;
  if (ctorId) {
    const made = !!store.cardEls[ctorId];
    const inherited = card.ctor_owner && card.ctor_owner !== card.qualname;
    const box = document.createElement("div");
    box.className = "acc";
    const head = document.createElement("div");
    head.className = "acc-head";
    head.dataset.method = ctorId;
    head.innerHTML = '<span class="caret">' + icon("enter") + '</span>' +
      '<span class="nm">' + esc(store.NODES[ctorId].title) + '</span>' +
      '<span class="cnt">' +
      (inherited ? "继承自 " + esc(card.ctor_owner) : "构造") + '</span>';
    head.title = made ? "定位到它的卡片" : "展开为卡片";
    head.onclick = function (e) {
      e.stopPropagation();
      if (store.cardEls[ctorId]) { select(ctorId); relayout(); }
      else expand(ctorId, classId);
    };
    box.appendChild(head);
    host.appendChild(box);
  } else if (instantiated && card.external_base) {
    // 构造来自项目外的基类：显示父类的 __init__ 名字，但不可展开
    const box = document.createElement("div");
    box.className = "acc";
    const head = document.createElement("div");
    head.className = "acc-head";
    // 不可展开的一行：地球＝构造来自项目外，和上面那行的 ↳ 形成"能进去 / 进不去"的对比
    head.innerHTML = '<span class="caret">' + icon("external") + '</span>' +
      '<span class="nm">' + esc(card.external_base) + '.__init__(…)</span>' +
      '<span class="cnt">构造 · 项目外</span>';
    head.title = card.external_base +
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
      const made = !!store.cardEls[mid];
      const box = document.createElement("div");
      box.className = "acc";
      const head = document.createElement("div");
      head.className = "acc-head";
      head.dataset.method = mid;
      head.innerHTML = '<span class="caret">' + icon("enter") + '</span>' +
        '<span class="nm">' + esc(m.qualname.split(".").pop()) +
        '(' + m.params.join(", ") + ')</span>' +
        '<span class="cnt">被调 ' + (store.FANIN[mid] || 0) + '</span>';
      head.title = made ? "定位到它的卡片" : "展开为卡片";
      head.onclick = function (e) {
        e.stopPropagation();
        expand(mid, classId);
      };
      box.appendChild(head);
      host.appendChild(box);
    });
  } else {
    const note = document.createElement("div");
    note.className = "hidden-note";
    note.textContent = "还没有走过本类的方法（目录只收录走过的，点行尾标记指进来才补）";
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

