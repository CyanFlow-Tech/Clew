// shell/settings.js
import { ICON, normPath } from "./util.js";

import { ST_TEXT, directoryBlock, spacePanes } from "./settings-block.js";

/// 一页里两块：都是目录，语义挨着，所以放同一个导航项下用标题分开。
export function settingsPage() {
  const box = document.createElement("div");
  box.className = "st-page";

  const blocks = {};
  const page = { el: box };
  page.refreshBlock = function (kind) {
    blocks[kind].refresh();
  };
  page.refresh = function () {
    blocks.data.refresh();
    blocks.cache.refresh();
  };

  const notify = function (kind) {
    for (const other of spacePanes) {
      if (other !== page) other.refreshBlock(kind);
    }
  };

  blocks.data = directoryBlock("data", notify);
  blocks.cache = directoryBlock("cache", notify);
  box.appendChild(blocks.data.el);
  box.appendChild(blocks.cache.el);

  spacePanes.push(page);
  return page;
}

// 弹窗：欢迎页与主窗口共用
export const settingsModal = document.createElement("div");
settingsModal.id = "settings";
settingsModal.hidden = true;
settingsModal.innerHTML =
  '<div class="st-box">' +
    '<div class="st-bar"><span class="st-title">' + ST_TEXT.title + "</span>" +
      '<button type="button" class="st-x" data-k="close">' + ICON.close + "</button></div>" +
    '<div class="st-main">' +
      '<nav class="st-nav">' +
        '<button type="button" class="st-nav-item on">' + ST_TEXT.nav + "</button>" +
      "</nav>" +
      '<div class="st-content"></div>' +
    "</div>" +
  "</div>";
document.body.appendChild(settingsModal);

export const modalPage = settingsPage();
settingsModal.querySelector(".st-content").appendChild(modalPage.el);

settingsModal.querySelector('[data-k="close"]').onclick = closeSettings;
settingsModal.addEventListener("click", function (e) {
  if (e.target === settingsModal) closeSettings();
});
document.addEventListener("keydown", function (e) {
  if (e.key === "Escape" && !settingsModal.hidden) closeSettings();
});

export function openSettings() {
  settingsModal.hidden = false;
  modalPage.refresh();
}

export function closeSettings() {
  settingsModal.hidden = true;
}






