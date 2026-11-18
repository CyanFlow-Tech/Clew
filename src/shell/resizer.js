// shell/resizer.js
import { side } from "./sidebase.js";

// ── 侧栏宽度可拖 ────────────────────────────────────────────────
// 宽度只写进 --side-w，画布、面包屑行、底部提示都跟着这个变量走。
// 拖的时候只在 rAF 里通知一次画布重排，不然每移动 1px 就重算一遍卡片。
export const SIDE_W_KEY = "clew.sideWidth";
export const SIDE_W_MIN = 200;
export const resizer = document.createElement("div");
resizer.id = "side-resize";
side.appendChild(resizer);

export let sideWidth = 272;
try {
  const saved = Number(localStorage.getItem(SIDE_W_KEY));
  if (saved >= SIDE_W_MIN) sideWidth = saved;
} catch (err) {
  /* 读不出来就用默认 */
}

export function clampWidth(px) {
  const max = Math.max(SIDE_W_MIN + 40, Math.round(window.innerWidth * 0.6));
  return Math.max(SIDE_W_MIN, Math.min(Math.round(px), max));
}

export let relayoutQueued = false;
export function queueRelayout() {
  if (relayoutQueued) return;
  relayoutQueued = true;
  requestAnimationFrame(function () {
    relayoutQueued = false;
    window.dispatchEvent(new Event("resize"));
  });
}

export function applyWidth(px, save) {
  sideWidth = clampWidth(px);
  document.documentElement.style.setProperty("--side-w", sideWidth + "px");
  if (save) {
    try {
      localStorage.setItem(SIDE_W_KEY, String(sideWidth));
    } catch (err) {
      /* 隐私模式写不进就算了 */
    }
  }
  queueRelayout();
}
applyWidth(sideWidth, false);

resizer.addEventListener("pointerdown", function (e) {
  e.preventDefault();
  resizer.setPointerCapture(e.pointerId);
  resizer.classList.add("dragging");
  document.body.classList.add("side-resizing");
});
resizer.addEventListener("pointermove", function (e) {
  if (!resizer.classList.contains("dragging")) return;
  // 面板左边界固定在活动栏右侧，宽度就是指针横坐标减掉这一段
  applyWidth(e.clientX - 40, false);
});
export function endDrag(e) {
  if (!resizer.classList.contains("dragging")) return;
  resizer.classList.remove("dragging");
  document.body.classList.remove("side-resizing");
  try {
    resizer.releasePointerCapture(e.pointerId);
  } catch (err) {
    /* 指针已经释放了 */
  }
  applyWidth(sideWidth, true);
}
resizer.addEventListener("pointerup", endDrag);
resizer.addEventListener("pointercancel", endDrag);
