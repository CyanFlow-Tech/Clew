// shell/siderows.js
import { store } from "./store.js";
import { ICON, basename } from "./util.js";
import { STAR_SVG } from "../shared/icons.js";
import { esc, markHit, showQual } from "../shared/format.js";
import { shortName } from "./id.js";
import { favorites } from "./favorites.js";
import { st } from "./sidebar.js";
import { spList } from "./sidebase.js";
import { ensureEntry } from "./viewmenu.js";
import { collectRows, emitDir } from "./dirtree.js";

export const CARET_SHUT = ICON.chevronRight;
export const CARET_OPEN = ICON.chevronDown;

export function spRowEl(r) {
  const el = document.createElement("div");
  el.className =
    "sp-row" + (r.kind === "note" ? " note" : "") + (r.third ? " third" : "");
  el.style.paddingLeft = 6 + r.depth * 11 + "px";
  el.dataset.depth = String(r.depth);
  if (r.kind === "dir") {
    el.dataset.dir = r.dir;
    el.title = r.dir;
    el.innerHTML =
      '<span class="caret" data-act="caret">' + (r.open ? CARET_OPEN : CARET_SHUT) + "</span>" +
      '<span class="ic">' + ICON.folder + "</span>" +
      '<span class="nm">' + esc(r.name) + "</span>";
    return el;
  }
  if (r.kind === "file") {
    el.dataset.mod = r.module;
    el.title = r.path;
    el.innerHTML =
      '<span class="caret" data-act="caret">' + (r.open ? CARET_OPEN : CARET_SHUT) + "</span>" +
      '<span class="nm">' + esc(basename(r.path)) + "</span>";
    return el;
  }

  const active = window.getActiveRoot ? window.getActiveRoot() : null;
  if (active && active === r.card) el.classList.add("sel");
  const star = favorites.has(r.card);
  const flagHtml =
    '<span class="flag ' + (star ? "on" : "off") + '" data-fav="' + esc(r.card) +
    '" title="标记入口">' + STAR_SVG + "</span>";

  // 一行函数名，一行文件名当注释：一行硬塞两个名字在窄面板里必然被截断
  if (r.kind === "entry") {
    el.dataset.card = r.card;
    el.dataset.module = r.module || "";
    el.title = (r.file ? r.file : r.module) + ":" + showQual(r.qual);
    el.innerHTML =
      '<span class="caret"></span>' +
      '<span class="ic">\u0192</span>' +
      '<span class="nm fn">' +
      (r.hot && r.hotName ? markHit(shortName(r.card), r.hotName) : esc(shortName(r.card))) +
      "</span>" +
      '<span class="tail">' + flagHtml + "</span>";
    return el;
  }
  if (r.kind === "more") {
    el.dataset.more = "1";
    el.innerHTML = '<span>加载更多</span>';
    return el;
  }
  if (r.kind === "note") {
    // 文件名行缩进到函数名文字那一列：caret 10 + 间隙 5 + 图标 11 + 间隙 5
    // 点它跟点上面那行函数一样，打开同一个符号
    el.style.paddingLeft = 6 + r.depth * 11 + 31 + "px";
    el.dataset.card = r.card;
    el.innerHTML =
      '<span class="note-file">' +
      (r.hot && r.hotPath ? markHit(r.file, r.hotPath) : esc(r.file)) +
      "</span>";
    return el;
  }

  const caret = r.kids
    ? '<span class="caret" data-act="caret">' + (r.open ? CARET_OPEN : CARET_SHUT) + "</span>"
    : '<span class="caret"></span>';
  const ic = r.kindName === "class" ? "C" : r.kindName === "method" ? "M" : "\u0192";
  el.dataset.card = r.card;
  el.dataset.module = r.module || "";
  el.title = r.module + ":" + showQual(r.qual || "");
  el.innerHTML =
    caret +
    '<span class="ic">' + ic + "</span>" +
    '<span class="nm">' + esc(shortName(r.card)) + "</span>" +
    (r.isEntry ? '<span class="edot" title="自动识别的入口"></span>' : "") +
    '<span class="tail">' +
      (r.showModule ? '<span class="mod">' + esc(r.module) + "</span>" : "") +
      flagHtml +
    "</span>";
  return el;
}

// 行数可能到几万（索引了整个标准库那种），一屏只画需要的那几张，
// 滚到底再补下一批：DOM 数量跟着视口走，不跟着数据量走。
export const SP_PAGE = 200;
export let spRows = [];
export let spDrawn = 0;

export function drawMore() {
  if (spDrawn >= spRows.length) return;
  const end = Math.min(spDrawn + SP_PAGE, spRows.length);
  const frag = document.createDocumentFragment();
  for (; spDrawn < end; spDrawn++) frag.appendChild(spRowEl(spRows[spDrawn]));
  spList.appendChild(frag);
}

spList.addEventListener("scroll", function () {
  if (spList.scrollTop + spList.clientHeight >= spList.scrollHeight - 400) drawMore();
});

export async function spRender() {
  if (!store.sideOpen) return;
  const token = ++store.spSeq;
  if (store.spView === "entry") await ensureEntry();
  if (token !== store.spSeq) return;

  const rows = collectRows();
  spList.innerHTML = "";
  spRows = rows;
  spDrawn = 0;
  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "sp-empty";
    empty.textContent =
      store.spView === "fav"
        ? "给定义按 ★ 就会出现在这里"
        : store.spView === "recent"
          ? "在卡片里操作过的函数会排在这里"
        : store.spView === "entry"
          ? "扫描器没有找到入口"
          : "没有定义";
    spList.appendChild(empty);
    return;
  }
  drawMore();
}

// 展开收起就地增删那几行，不重画整棵树：一个文件几百行定义时，
// 每点一次都重建全部 DOM 会把展开变成肉眼可见的卡顿。
export function dropChildren(rowEl, keepDepth) {
  let n = rowEl.nextElementSibling;
  while (n && Number(n.dataset.depth) > keepDepth) {
    const next = n.nextElementSibling;
    n.remove();
    n = next;
  }
}

// 插入也分批：一个大目录里可能有好几百个文件，一次 append 完会让点击
// 到画面更新之间空一拍。每帧补一批，插完为止。
export function appendRows(anchor, rows) {
  let i = 0;
  let last = anchor;
  function step() {
    // 锚点被移除（视图切了、父节点收起了）就停下来，别往野节点里塞
    if (!last.isConnected) return;
    const frag = document.createDocumentFragment();
    const end = Math.min(i + SP_PAGE, rows.length);
    for (; i < end; i++) frag.appendChild(spRowEl(rows[i]));
    const tail = frag.lastChild;
    last.after(frag);
    last = tail;
    if (i < rows.length) requestAnimationFrame(step);
  }
  step();
}

export function toggleDir(key) {
  const mine = st();
  const wasOpen = mine.dirs.has(key);
  if (wasOpen) mine.dirs.delete(key);
  else mine.dirs.add(key);
  const el = spList.querySelector('[data-dir="' + key + '"]');
  if (!el || !store.spTree || !store.spTree.nodes.has(key)) {
    spRender();
    return;
  }
  const depth = Number(el.dataset.depth);
  dropChildren(el, depth);
  el.querySelector('[data-act="caret"]').innerHTML = wasOpen
    ? CARET_SHUT
    : CARET_OPEN;
  if (wasOpen) return;
  const rows = [];
  emitDir(store.spTree.nodes.get(key), key, depth + 1, rows);
  appendRows(el, rows);
}

