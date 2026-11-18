// shell/filemenu.js
import { h } from "../shared/dom.js";
import { ICON } from "./util.js";
import { burger, menu, projectButton, row1 } from "./titlebar.js";
import { closeProjectPop, projectPop } from "./projectpanel.js";

// ── 文件菜单 ────────────────────────────────────────────────────
export const pop = document.createElement("div");
pop.className = "tb-pop";
pop.hidden = true;
document.body.appendChild(pop);

// 右侧子菜单：项目列表用它展开，跟主下拉分开一层
export const subPop = document.createElement("div");
subPop.className = "tb-pop sub-pop";
subPop.hidden = true;
document.body.appendChild(subPop);

export function closeSub() {
  subPop.hidden = true;
  subPop.innerHTML = "";
}

export function closePop() {
  closeSub();
  pop.hidden = true;
  for (const b of menu.children) b.classList.remove("open");
}

export function openSub(anchor, items) {
  subPop.innerHTML = "";
  for (const item of items) {
    if (item === "-") {
      const s = h("div", { class: "sep" });
      subPop.appendChild(s);
      continue;
    }
    const b = document.createElement("button");
    if (item.icon) {
      const mark = h("span", { class: "mi" });
      mark.innerHTML = item.icon;
      b.appendChild(mark);
    }
    b.appendChild(document.createTextNode(item.label));
    if (item.disabled) b.disabled = true;
    if (item.title) b.title = item.title;
    b.onclick = function (e) {
      e.stopPropagation();
      if (item.disabled) return;
      closePop();
      item.run();
    };
    subPop.appendChild(b);
  }
  const r = anchor.getBoundingClientRect();
  subPop.style.left = Math.round(r.right + 4) + "px";
  subPop.style.top = Math.round(r.top) + "px";
  subPop.hidden = false;
}

export function popAt(anchor, items) {
  pop.innerHTML = "";
  for (const item of items) {
    if (item === "-") {
      const s = h("div", { class: "sep" });
      pop.appendChild(s);
      continue;
    }
    const b = document.createElement("button");
    if (item.icon) {
      const mark = h("span", { class: "mi" });
      mark.innerHTML = item.icon;
      b.appendChild(mark);
    }
    b.appendChild(document.createTextNode(item.label));
    if (item.sub) {
      b.classList.add("has-sub");
      const arrow = document.createElement("span");
      arrow.innerHTML = ICON.chevronRight;
      b.appendChild(arrow);
      b.onmouseenter = function () { openSub(b, item.sub()); };
      b.onclick = function (e) {
        e.stopPropagation();
        openSub(b, item.sub());
      };
    } else {
      if (item.disabled) b.disabled = true;
      b.onclick = function () {
        closePop();
        item.run();
      };
    }
    pop.appendChild(b);
  }
  const r = anchor.getBoundingClientRect();
  pop.style.left = Math.round(r.left) + "px";
  pop.hidden = false;
}

// ── 菜单栏模式 ──────────────────────────────────────────────────
export let menuMode = false;
export function setMenuMode(on) {
  menuMode = on;
  menu.hidden = !on;
  burger.style.display = on ? "none" : "";
  projectButton.style.display = on ? "none" : "";
  if (!on) closePop();
}

burger.onclick = function (e) {
  e.stopPropagation();
  setMenuMode(!menuMode);
};

document.addEventListener("keydown", function (e) {
  if (e.key !== "Escape") return;
  if (!subPop.hidden) closeSub();
  else if (!projectPop.hidden) closeProjectPop();
  else if (!pop.hidden) closePop();
  else if (menuMode) setMenuMode(false);
});

// 点任何地方都收起下拉；用捕获阶段，免得被标题栏元素吃掉
window.addEventListener(
  "mousedown",
  function (e) {
    if (
      !projectPop.hidden &&
      !projectPop.contains(e.target) &&
      !projectButton.contains(e.target)
    ) {
      closeProjectPop();
    }
    if (!pop.hidden && !pop.contains(e.target) && !subPop.contains(e.target)) {
      closePop();
    }
    if (menuMode && !row1.contains(e.target) && !pop.contains(e.target)) {
      setMenuMode(false);
    }
  },
  true,
);
