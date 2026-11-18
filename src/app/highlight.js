// app/highlight.js
import { esc } from "../shared/format.js";
import { icon } from "../shared/icons.js";
import { ensure } from "./data.js";
import { store } from "./store.js";
import { renderCard } from "./card.js";
import { expand } from "./place.js";
import { relayout } from "./layout.js";
import { hideBubble, hoverCs, showBubble } from "./edge.js";

// ─── 卡片渲染 ───────────────────────────────────────────────────



// ─── 行内高亮 ─────────────────────────────────────────────────────
// 卡片里的一行是合成出来的（`%0 = f(x)`），不是原始源码。高亮交给
// highlight.js，只有临时量 `%N` 不合法，先遮成合法标识符，渲染完再换回来。
export const TMP_MASK = /%(\d+)/g;
export const TMP_BACK = /__clew(\d+)__/g;

export function highlightLine(text) {
  const hljs = window.hljs;
  if (!hljs) return null;
  const masked = text.replace(TMP_MASK, "__clew$1__");
  try {
    return {
      html: hljs.highlight(masked, { language: "python", ignoreIllegals: true }).value,
      masked: masked !== text,
    };
  } catch (err) {
    return null; // 高亮失败就当没有，别把卡片搞没了
  }
}

export function textNodesOf(root) {
  const out = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
  while (walker.nextNode()) out.push(walker.currentNode);
  return out;
}

// 把遮住的临时量还原成 %N，给它自己的颜色
export function restoreTemps(row) {
  textNodesOf(row).forEach(function (node) {
    const text = node.nodeValue;
    if (text.indexOf("__clew") < 0) return;
    const frag = document.createDocumentFragment();
    let last = 0;
    let m;
    TMP_BACK.lastIndex = 0;
    while ((m = TMP_BACK.exec(text))) {
      if (m.index > last) frag.appendChild(document.createTextNode(text.slice(last, m.index)));
      const span = document.createElement("span");
      span.className = "tmp";
      span.textContent = "%" + m[1];
      frag.appendChild(span);
      last = m.index + m[0].length;
    }
    if (last < text.length) frag.appendChild(document.createTextNode(text.slice(last)));
    node.parentNode.replaceChild(frag, node);
  });
}

// 调用的目标名字挂一条虚线，来源是外部模块时带 tooltip。
// 按文本节点找，不去动 hljs 生成的标记结构。
export function markExternal(row, target, imported) {
  const escaped = target.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp("\\b" + escaped + "\\b");
  const nodes = textNodesOf(row);
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    const parent = node.parentNode;
    if (!parent || !node.nodeValue) continue;
    if (parent.classList && parent.classList.contains("ext")) continue;
    const m = re.exec(node.nodeValue);
    if (!m) continue;
    const span = document.createElement("span");
    span.className = "ext";
    if (imported) span.title = imported;
    span.textContent = m[0];
    const frag = document.createDocumentFragment();
    const text = node.nodeValue;
    if (m.index > 0) frag.appendChild(document.createTextNode(text.slice(0, m.index)));
    frag.appendChild(span);
    if (m.index + m[0].length < text.length) {
      frag.appendChild(document.createTextNode(text.slice(m.index + m[0].length)));
    }
    parent.replaceChild(frag, node);
    return;
  }
}

export function renderLine(ln, host, ownerId) {
  const row = document.createElement("div");
  row.className = "ln " + ln.k;
  row.style.paddingLeft = (34 + (ln.indent || 0) * 12) + "px";
  const c = ln.cs ? store.CALLS[ln.cs] : null;
  const lit = highlightLine(ln.text);
  if (lit) {
    row.innerHTML = lit.html;
    if (lit.masked) restoreTemps(row);
  } else {
    row.innerHTML = esc(ln.text);
  }
  if (c && c.imported && c.target) markExternal(row, c.target, c.imported);
  if (ln.ln) {
    const num = document.createElement("span");
    num.className = "lnum";
    num.textContent = ln.ln;
    row.appendChild(num);
  }
  if (c && c.to) {
    const a = document.createElement("span");
    let cls = "anchor";
    if (c.conf === "heur") cls += " heur";
    else if (c.conf === "poly") cls += " poly";
    else if (store.cardEls[c.to]) cls += " done";
    a.className = cls;
    a.dataset.cs = ln.cs;
    const cnt = c.candidates && c.candidates.length > 1
      ? (c.candidates.length > 9 ? "+" : c.candidates.length) : "";
    a.innerHTML = icon("enter") + (cnt ? '<span class="cnt">' + cnt + "</span>" : "");
    if (store.selectedCs === ln.cs) a.className += " on";
    a.title = "展开并显示这一跳的实参绑定";
    a.onmouseenter = function (ev) {
      showBubble(ln.cs, ev);
      if (c.to && !store.NODES[c.to]) {
        // 形参在目标卡里，取回来再把气泡画一遍
        ensure([c.to]).then(function () {
          if (hoverCs === ln.cs) showBubble(ln.cs, ev);
        });
      }
    };
    a.onmouseleave = hideBubble;
    a.onclick = function (ev) {
      ev.stopPropagation();
      hideBubble();
      store.selectedCs = ln.cs;
      ensure(c.to ? [c.to] : []).then(function () {
        if (ownerId && store.cardEls[ownerId]) { renderCard(ownerId); relayout(); }
        expand(c.to, c.from, ln.cs);
      });
    };
    row.appendChild(a);
  } else if (c && c.candidates && c.candidates.length > 1) {
    // 接收者类型不知道，同名方法有一堆：不假装解析到了某一个，
    // 给出候选让人自己挑。悬停或点击都把气泡打开。
    const a = document.createElement("span");
    a.className = "anchor poly";
    a.innerHTML =
      icon("enter") +
      '<span class="cnt">' +
      (c.candidates.length > 9 ? "+" : c.candidates.length) +
      "</span>";
    if (store.selectedCs === ln.cs) a.className += " on";
    a.title = "接收者类型未知，同名方法有 " + c.candidates.length + " 个候选，点开挑一个";
    const load = function (ev) {
      showBubble(ln.cs, ev);
      ensure(c.candidates).then(function () {
        if (hoverCs === ln.cs) showBubble(ln.cs, ev);
      });
    };
    a.onmouseenter = load;
    a.onmouseleave = hideBubble;
    a.onclick = function (ev) {
      ev.stopPropagation();
      load(ev);
    };
    row.appendChild(a);
  } else if (c) {
    const a = document.createElement("span");
    a.className = "anchor dead";
    a.innerHTML = icon("enter");
    a.title = "未解析（" + (c.conf === "builtin" ? "内置/库函数" : "找不到定义") + "）";
    row.appendChild(a);
  }
  host.appendChild(row);

  // 被选中的调用：下一行插入实参绑定注释
  if (c && store.selectedCs === ln.cs) {
    const box = document.createElement("div");
    box.className = "bind";
    box.textContent = bindingText(c);
    host.appendChild(box);
  }
}

// 完整绑定文本：所有能对上的形参，换行不了的自动折行
export function bindingText(c) {
  const to = store.NODES[c.to];
  if (!to) return "";
  const params = to.params || [];
  if (!params.length) return "（无参数）";
  const parts = params.map(function (p) {
    const v = (c.args || {})[p];
    if (v === undefined) return p + " ← ?";
    return p + " ← " + v;
  });
  return parts.join("      ");
}

