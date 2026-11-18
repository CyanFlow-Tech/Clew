// shell/help.js
import { store } from "./store.js";
import { VERSION } from "./boot.js";
import { ICON, recent } from "./util.js";
import { icon } from "../shared/icons.js";
import { menu } from "./titlebar.js";
import { popAt } from "./filemenu.js";
import { chooseFolder, closeProject, openPath, rebuildIndex, removeIndex } from "./project.js";

// ── 帮助 ────────────────────────────────────────────────────────
export const help = document.createElement("div");
help.id = "tb-help";
help.hidden = true;
help.innerHTML =
  '<div class="card"><span class="close">' + ICON.close + '</span>' +
  "<h3>Clew " + VERSION + "</h3>" +
  "<p>从入口开始读调用图。点行尾的 <span class=\"hi\">" + icon("enter") +
  "</span> 标记进入实现，类卡片按需长出方法目录。</p>" +
  "<p>空白处按住拖动画布，右下角 minimap 点哪儿跳哪儿。</p>" +
  "<p>第二行下拉换入口；文件菜单里可以打开或关闭项目。</p></div>";
document.body.appendChild(help);
help.querySelector(".close").onclick = function () {
  help.hidden = true;
};
help.onclick = function (e) {
  if (e.target === help) help.hidden = true;
};

// 菜单项：点一下展开，鼠标划过去也展开（不用再点第二次）
menu.addEventListener("click", function (e) {
  const item = e.target.closest("[data-menu]");
  if (!item) return;
  openMenu(item);
});
for (const item of menu.children) {
  item.onmouseenter = function () {
    if (menu.hidden) return;
    openMenu(item);
  };
}

/// 展开某一栏的菜单。三栏都有下拉，帮助那一栏只有一项。
export function openMenu(item) {
  for (const b of menu.children) b.classList.toggle("open", b === item);
  if (item.dataset.menu === "file") {
    popAt(item, [
      { label: "打开项目…", run: chooseFolder },
      { label: "项目列表", sub: projectListItems },
      "-",
      { label: "关闭项目", run: closeProject },
      "-",
      { label: "重建索引", run: rebuildIndex },
      { label: "删除索引", run: removeIndex },
    ]);
  } else if (item.dataset.menu === "theme") {
    popAt(item, themeItems());
  } else {
    popAt(item, [
      {
        label: "关于 Clew",
        run: function () {
          help.hidden = false;
        },
      },
    ]);
  }
}

// 代码配色主题：直接铺在菜单里，当前那个前面打勾
export function themeItems() {
  const list = window.CLEW_THEMES || [];
  const now = window.__clewThemeId ? window.__clewThemeId() : "";
  const items = list.map((t) => ({
    label: t.name,
    icon: t.id === now ? icon("check") : "",
    run: () => {
      if (window.__clewApplyTheme) window.__clewApplyTheme(t.id);
    },
  }));
  if (!items.length) items.push({ label: "主题表没加载", disabled: true });
  return items;
}

// 项目列表子菜单：固定显示前几个，其余折成一行省略号，最后是管理入口
export const SUB_MENU_MAX = 8;
export function projectListItems() {
  const list = recent()
    .filter((p) => !p.hidden)
    .filter((p) => p.path !== store.currentPath);
  const items = list.slice(0, SUB_MENU_MAX).map((p) => ({
    label: p.name,
    run: () => openPath(p.path),
  }));
  if (list.length > SUB_MENU_MAX) {
    items.push({
      label: "…还有 " + (list.length - SUB_MENU_MAX) + " 个",
      disabled: true,
    });
  }
  if (!items.length) items.push({ label: "没有其他项目", disabled: true });
  items.push("-");
  items.push({ label: "管理项目", disabled: true, title: "稍后提供" });
  return items;
}

