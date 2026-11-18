// shared/dom.js
// 建节点只有这一个口子：标签 + 属性 + 内部标记。事件仍在拿到节点之后再挂，
// 因为多数地方要先查询到子节点（如单击按钮）才能接线。
// 叫 h 而不是 el：代码里 el 到处都是「手里那个节点」的变量名，会撞。

/// 属性里这几个走特殊处理，其余按 DOM 属性/特性落：
/// class → className，html/text → 内容，dataset → data-*，style → 行内样式
export function h(tag, props, html) {
  const node = document.createElement(tag);
  const p = props || {};
  Object.keys(p).forEach(function (key) {
    const value = p[key];
    if (value === undefined || value === null || value === false) return;
    if (key === "class") node.className = value;
    else if (key === "html") node.innerHTML = value;
    else if (key === "text") node.textContent = value;
    else if (key === "dataset") {
      Object.keys(value).forEach(function (name) { node.dataset[name] = value[name]; });
    } else if (key === "style") {
      if (typeof value === "string") node.setAttribute("style", value);
      else Object.keys(value).forEach(function (name) { node.style[name] = value[name]; });
    } else if (key in node) node[key] = value;
    else node.setAttribute(key, value);
  });
  if (html !== undefined) node.innerHTML = html;
  return node;
}
