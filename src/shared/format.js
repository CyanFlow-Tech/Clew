// shared/format.js
// 画布和外壳都要用的纯字符串函数：不碰 DOM，不持有状态。
// 拆分前这些在 app.js 与 shell.js 里各有一份，改一处忘另一处就出现两种显示。

export function esc(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/// 库里的 <module> 就是 if __name__ == "__main__" 那一块
export function showQual(qual) {
  return qual === "<module>" ? "__main__" : qual;
}

/// 卡片 id（module:qualname）的显示形式
export function showId(id) {
  const s = String(id);
  const i = s.indexOf(":");
  if (i < 0) return s;
  return s.slice(0, i) + ":" + showQual(s.slice(i + 1));
}

/// 把命中的那一段包成 <b>，只用在排第一的结果上
export function markHit(text, needle) {
  const t = String(text || "");
  const n = String(needle || "").toLowerCase();
  if (!n) return esc(t);
  const i = t.toLowerCase().indexOf(n);
  if (i < 0) return esc(t);
  return esc(t.slice(0, i)) +
    '<span class="hit">' + esc(t.slice(i, i + n.length)) + "</span>" +
    esc(t.slice(i + n.length));
}
