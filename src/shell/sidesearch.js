// shell/sidesearch.js
import { h } from "../shared/dom.js";
import { store } from "./store.js";
import { api } from "./boot.js";
import { spListSearch, spQName, spQPath } from "./sidebase.js";
import { spRowEl } from "./siderows.js";

// ── 搜索视图 ────────────────────────────────────────────────────
// 两个条件各自独立：路径里含这串、函数名里含这串，同时成立才留下。
// 分页在背后是 offset/limit，前端只记已经拿到多少条。
export const SP_PAGE_SIZE = 50;
export const spSearch = { total: 0, truncated: false, hits: [], busy: false, timer: null };
export let spSearchSeq = 0;

export function spQueryText() {
  return { path: spQPath.value.trim(), name: spQName.value.trim() };
}

export async function spSearchPage(append) {
  if (!store.currentPath) return;
  if (spSearch.busy) return;
  spSearch.busy = true;
  const seq = ++spSearchSeq;
  const q = spQueryText();
  const offset = append ? spSearch.hits.length : 0;
  try {
    const page = await api.invoke("search_symbols", {
      qPath: q.path,
      qName: q.name,
      offset: offset,
      limit: SP_PAGE_SIZE,
    });
    if (seq !== spSearchSeq) return;
    spSearch.total = page.total || 0;
    spSearch.truncated = !!page.truncated;
    spSearch.hits = append ? spSearch.hits.concat(page.hits || []) : page.hits || [];
  } catch (err) {
    if (seq !== spSearchSeq) return;
    if (window.__clewLog) window.__clewLog("search_symbols 失败：" + err);
    spSearch.hits = [];
    spSearch.total = 0;
  } finally {
    spSearch.busy = false;
  }
  if (seq === spSearchSeq) spSearchRender();
}

/// 搜索面板自己的渲染：一页 50 条，不够就末尾挂一行「加载更多」
export function spSearchRender() {
  const q = spQueryText();
  const rows = [];
  spSearch.hits.forEach(function (h, i) {
    rows.push({
      kind: "entry",
      depth: 0,
      card: h.card_id,
      module: h.module,
      qual: h.qualname,
      file: "",
      line: 0,
      // 只有排第一的那行高亮命中片段，其余保持安静
      hot: i === 0 && !!(q.name || q.path),
      hotName: q.name,
      hotPath: q.path,
    });
    rows.push({ kind: "note", depth: 0, card: h.card_id, file: h.path });
  });
  if (spSearch.hits.length < spSearch.total) rows.push({ kind: "more", depth: 0 });

  spListSearch.innerHTML = "";
  if (!rows.length) {
    const empty = h("div", { class: "sp-empty", text: spSearch.busy ? "" : "没有匹配" });
    spListSearch.appendChild(empty);
    return;
  }
  const frag = document.createDocumentFragment();
  rows.forEach((r) => frag.appendChild(spRowEl(r)));
  spListSearch.appendChild(frag);
}
