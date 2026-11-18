// shell/sidebar.js
import { store } from "./store.js";
import { ICON } from "./util.js";
import { openSettings } from "./settings.js";

// ── 侧边栏：符号树 ──────────────────────────────────────────────
// 仿 PyCharm：最左边一条活动栏，右边一块面板。面板顶部是视图下拉，
// 全部函数 / 入口函数 / 收藏夹。入口函数是扫描器自己认出来的，
// 收藏夹是用户按 ★ 收起来的内容。搜索不在这里，它在独立的入口面板里。
//
// 渲染只画展开的那几层：文件默认收起，所以一个项目几千个定义也只有几百行
// DOM。展开时按帧分批补，一次 append 两百行，长文件也不会卡住。
export const SP_VIEWS = [
  { id: "all", label: "全部函数" },
  { id: "entry", label: "入口函数" },
  { id: "fav", label: "收藏夹" },
  { id: "recent", label: "最近使用" },
];
export const SP_VIEW_KEY = "clew.sideView";
try {
  const saved = localStorage.getItem(SP_VIEW_KEY);
  if (SP_VIEWS.some((v) => v.id === saved)) store.spView = saved;
} catch (err) {
  /* 读不出来就用默认 */
}

// 侧栏的数据按需取：先只有文件行，展开哪个文件才问哪个文件的定义。
// 一上来就把整棵树拉过来的话，标准库那种规模是七十万个符号、上百兆 JSON，
// 面板还没画出来就先卡住了。
// 三个视图各留一套展开状态：在「全部函数」里翻到的位置，切到「入口函数」
// 再切回来还在。默认值按视图量级定，全部函数全收起，另外两个全展开。
export function spBlank() {
  return { dirs: new Set(), files: new Set(), syms: new Set() };
}
export const spStates = {
  all: spBlank(),
  entry: spBlank(),
  fav: spBlank(),
  recent: spBlank(),
};

export function st() {
  return spStates[store.spView] || spStates.all;
}

// 活动栏就是侧栏的 tab：一个符号树，一个搜索，点哪个切哪个。
// 最下面钉一个齿轮，进设置。
export const rail = document.createElement("div");
rail.id = "side-rail";
rail.innerHTML = `
  <button type="button" class="rail-btn on" data-tab="tree" title="符号树">${ICON.folder}</button>
  <button type="button" class="rail-btn" data-tab="search" title="搜索">${ICON.search}</button>
  <button type="button" class="rail-btn rail-pin" id="rail-settings" title="设置">${ICON.gear}</button>`;
document.body.appendChild(rail);

// 活动栏的齿轮：它不是 tab，别带 data-tab，否则会被面板的切换逻辑收走
rail.querySelector("#rail-settings").onclick = openSettings;


