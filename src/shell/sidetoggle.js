// shell/sidetoggle.js
import { store } from "./store.js";
import { api } from "./boot.js";
import { splitId } from "./id.js";
import { rail, spBlank, spStates, st } from "./sidebar.js";
import { side, spList, spQName, spQPath } from "./sidebase.js";
import { spSearch, spSearchPage } from "./sidesearch.js";
import { ensureSyms } from "./viewmenu.js";
import { emitNode, nestSyms, pushNested } from "./dirtree.js";
import { CARET_OPEN, CARET_SHUT, appendRows, dropChildren, spRender } from "./siderows.js";
import { applyTab } from "./sidetab.js";

// 取定义是异步的，展开期间再点同一条就等它插完再说，否则上一次的回包
// 会把已经收起的行又塞回来
export const spBusy = new Set();

export async function toggleFile(module) {
  if (spBusy.has(module)) return;
  const mine = st();
  const wasOpen = mine.files.has(module);
  if (wasOpen) mine.files.delete(module);
  else mine.files.add(module);
  const el = spList.querySelector('[data-mod="' + module + '"]');
  if (!el) {
    spRender();
    return;
  }
  const depth = Number(el.dataset.depth);
  dropChildren(el, depth);
  el.querySelector('[data-act="caret"]').innerHTML = wasOpen
    ? CARET_SHUT
    : CARET_OPEN;
  if (wasOpen) return;

  spBusy.add(module);
  try {
    const syms = store.spView === "entry" && store.spEntrySyms
      ? store.spEntrySyms.filter((s) => s.module === module)
      : await ensureSyms(module);
    // 取数期间可能已经点了别处、重渲染过，甚至又点了这一行
    if (!mine.files.has(module)) return;
    const host = spList.querySelector('[data-mod="' + module + '"]');
    if (!host) return;
    const rows = [];
    pushNested(rows, module, syms || [], Number(host.dataset.depth) + 1);
    appendRows(host, rows);
  } finally {
    spBusy.delete(module);
  }
}

export function toggleSym(card) {
  const mine = st();
  const wasOpen = mine.syms.has(card);
  if (wasOpen) mine.syms.delete(card);
  else mine.syms.add(card);
  const el = spList.querySelector('[data-card="' + card + '"]');
  if (!el) {
    spRender();
    return;
  }
  const depth = Number(el.dataset.depth);
  dropChildren(el, depth);
  el.querySelector('[data-act="caret"]').innerHTML = wasOpen
    ? CARET_SHUT
    : CARET_OPEN;
  if (wasOpen) return;
  // 只把这条定义自己的下一层挂上去；拿整份文件重算再截取的话，
  // 末尾会把它的兄弟节点一起塞进来
  const module = el.dataset.module || "";
  const qual = splitId(card).qualname;
  const syms =
    store.spView === "entry" && store.spEntrySyms
      ? store.spEntrySyms.filter((s) => s.module === module)
      : store.spCache.get(module) || [];
  const node = nestSyms(syms).byQ[qual];
  if (!node) return;
  const rows = [];
  node.kids.forEach((k) => emitNode(rows, k, depth + 1));
  appendRows(el, rows);
}

export function spReset() {
  store.spCache.clear();
  store.spFileByModule = new Map();
  spStates.all = spBlank();
  spStates.entry = spBlank();
  spStates.fav = spBlank();
  spStates.recent = spBlank();
  store.spTree = null;
  store.spEntrySyms = null;
}

export async function spLoad() {
  if (!store.currentPath) return;
  if (store.spLoadedFor === store.currentPath) {
    spRender();
    return;
  }
  store.spLoadedFor = null;
  try {
    const rows = await api.invoke("files");
    spReset();
    store.spFiles = rows || [];
    store.spFiles.forEach((f) => store.spFileByModule.set(f.module, f));
    store.spLoadedFor = store.currentPath;
  } catch (err) {
    // 索引还没建好时这里会失败，等建完 watchScan 会再叫一次
    if (window.__clewLog) window.__clewLog("files 取数失败：" + err);
    if (!store.spFiles) store.spFiles = [];
  }
  spRender();
}

export function setSideOpen(on) {
  store.sideOpen = on;
  side.hidden = !on;
  document.body.classList.toggle("side-open", on);
  if (on) {
    applyTab();
    spLoad();
  } else {
    // 面板收起来了，活动栏就不该还亮着一个 tab
    Array.prototype.forEach.call(rail.children, function (b) {
      b.classList.remove("on");
    });
  }
  // 画布宽度变了，卡片要重排；画布只监听 resize
  window.dispatchEvent(new Event("resize"));
}

export function spSearchReset() {
  clearTimeout(spSearch.timer);
  spSearch.timer = setTimeout(function () {
    spSearch.hits = [];
    spSearch.total = 0;
    spSearchPage(false);
  }, 250);
}
spQPath.addEventListener("input", spSearchReset);
spQName.addEventListener("input", spSearchReset);
