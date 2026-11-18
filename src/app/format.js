// app/format.js
// 卡片标题栏上的「全文 / 折叠」三态：按行数与当前状态算出来，不碰 DOM。
import { store } from "./store.js";
import { SUMMARY } from "./env.js";

export function hasMore(id) {
  return store.NODES[id].lines.length > SUMMARY;
}

export function toggleLabel(id, st) {
  if (st === "mini") return "展开";
  if (st === "full") return "折叠";
  return hasMore(id) ? "全文" : "折叠";
}

export function nextState(id, st) {
  if (st === "mini") return "summary";
  if (st === "full") return "mini";
  return hasMore(id) ? "full" : "mini";
}
