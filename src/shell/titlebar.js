// shell/titlebar.js
import { store } from "./store.js";
import { ICON, LOGO_IMG } from "./util.js";

// ── 标题栏 ──────────────────────────────────────────────────────
export const top = document.getElementById("top");
export const trail = document.getElementById("trail");

export const brand = document.createElement("span");
brand.className = "tb-brand";
brand.innerHTML = LOGO_IMG + '<span class="tb-title">Clew</span>';
store.brandTitle = brand.querySelector(".tb-title");

export const burger = document.createElement("button");
burger.className = "tb-icon";
burger.id = "tb-burger";
burger.innerHTML = ICON.menu;
burger.title = "菜单";

export const projectButton = document.createElement("button");
projectButton.className = "tb-project";
projectButton.id = "project-button";
projectButton.innerHTML =
  '<span class="pj-avatar"></span><span class="pj-name"></span>' +
  '<span class="pj-caret">' + ICON.chevronDown + "</span>";

export const menu = document.createElement("div");
menu.id = "tb-menu";
menu.hidden = true;
for (const [key, label] of [["file", "文件"], ["theme", "主题"], ["help", "帮助"]]) {
  const item = document.createElement("button");
  item.className = "tb-menu-item";
  item.dataset.menu = key;
  item.textContent = label;
  menu.appendChild(item);
}

export const status = document.createElement("span");
status.className = "tb-status";



export const row1 = document.createElement("div");
row1.id = "tb-row1";
row1.dataset.tauriDragRegion = "";
row1.appendChild(brand);
row1.appendChild(burger);
row1.appendChild(projectButton);
row1.appendChild(menu);

// 窗口控制按钮：系统标题栏已经关掉了，这三个是唯一的入口
{
  const winctl = document.createElement("div");
  winctl.id = "tb-winctl";
  winctl.innerHTML =
    '<button class="win-btn" data-win="min" title="最小化">' + ICON.min + "</button>" +
    '<button class="win-btn" data-win="max" title="最大化">' + ICON.max + "</button>" +
    '<button class="win-btn close" data-win="close" title="关闭">' + ICON.close + "</button>";
  row1.appendChild(winctl);

  const win = window.__TAURI__.window.getCurrentWindow();
  const maxBtn = winctl.querySelector('[data-win="max"]');

  // 最大化与还原是两个形状，跟着窗口状态换
  async function syncMaxButton() {
    try {
      const isMax = await win.isMaximized();
      maxBtn.innerHTML = isMax ? ICON.restore : ICON.max;
      maxBtn.title = isMax ? "向下还原" : "最大化";
    } catch {
      /* 窗口状态读不到就保持原样 */
    }
  }
  syncMaxButton();
  win.onResized(syncMaxButton);

  winctl.addEventListener("click", function (e) {
    const b = e.target.closest("[data-win]");
    if (!b) return;
    if (b.dataset.win === "min") win.minimize();
    else if (b.dataset.win === "max") win.toggleMaximize();
    else win.close();
  });
}

export const row2 = document.createElement("div");
row2.id = "tb-row2";
row2.dataset.tauriDragRegion = "";
row2.appendChild(trail);
row2.appendChild(status);

top.innerHTML = "";
top.dataset.tauriDragRegion = "";
top.appendChild(row1);
top.appendChild(row2);

/// 状态栏文字：进度条、错误、就绪都从这一处改
export function setStatus(text, isError) {
  status.classList.remove("busy");
  status.textContent = text || "";
  status.classList.toggle("err", !!isError);
}
