// shell/projectpanel.js
import { h } from "../shared/dom.js";
import { store } from "./store.js";
import { ICON, applyTitleTint, avatar, basename, colorFor, initials, normPath, recent } from "./util.js";
import { projectButton } from "./titlebar.js";
import { chooseFolder, openPath } from "./project.js";

// ── 项目面板 ────────────────────────────────────────────────────
export const projectPop = document.createElement("div");
projectPop.id = "project-pop";
projectPop.hidden = true;
document.body.appendChild(projectPop);

export function projectRow(p, onClick) {
  const row = h("div", { class: "pp-row" });
  const meta = h("div", { class: "pp-meta" });
  const name = h("div", { class: "pp-name", text: p.name });
  const path = h("div", { class: "pp-path", text: p.path });
  meta.appendChild(name);
  meta.appendChild(path);
  row.appendChild(avatar(p.name, "pp-avatar"));
  row.appendChild(meta);
  row.onclick = onClick;
  return row;
}

export function groupLabel(text) {
  const d = h("div", { class: "pp-group", text: text });
  return d;
}

export function sepLine() {
  const d = h("div", { class: "pp-sep" });
  return d;
}

export function renderProjectPop() {
  projectPop.innerHTML = "";

  const openItem = h("button", { class: "pp-item hl" });
  openItem.innerHTML = `<span class="ico">${ICON.folder}</span>打开新项目`;
  openItem.onclick = () => {
    closeProjectPop();
    chooseFolder();
  };
  projectPop.appendChild(openItem);
  projectPop.appendChild(sepLine());

  const list = recent();
  const current = list.find((p) => p.path === store.currentPath);
  const others = list.filter((p) => p.path !== store.currentPath);

  projectPop.appendChild(groupLabel("打开的项目"));
  if (current) {
    projectPop.appendChild(projectRow(current, () => closeProjectPop()));
  } else {
    const hint = h("div", { class: "pp-hint", text: "当前没有打开的项目" });
    projectPop.appendChild(hint);
  }

  if (others.length) {
    projectPop.appendChild(sepLine());
    projectPop.appendChild(groupLabel("最近的项目"));
    for (const p of others.slice(0, 12)) {
      projectPop.appendChild(
        projectRow(p, () => {
          closeProjectPop();
          openPath(p.path);
        }),
      );
    }
  }
}

export function openProjectPop() {
  renderProjectPop();
  const r = projectButton.getBoundingClientRect();
  projectPop.style.left = Math.round(r.left) + "px";
  projectPop.style.top = Math.round(r.bottom + 4) + "px";
  projectPop.hidden = false;
  projectButton.classList.add("open");
}

export function closeProjectPop() {
  projectPop.hidden = true;
  projectButton.classList.remove("open");
}

projectButton.onclick = function (e) {
  e.stopPropagation();
  if (projectPop.hidden) openProjectPop();
  else closeProjectPop();
};

export function renderProjectButton() {
  const nameEl = projectButton.querySelector(".pj-name");
  const avEl = projectButton.querySelector(".pj-avatar");
  if (store.currentPath) {
    const name = basename(store.currentPath);
    nameEl.textContent = name;
    avEl.textContent = initials(name);
    avEl.style.background = colorFor(name);
    projectButton.title = normPath(store.currentPath);
  } else {
    nameEl.textContent = "打开项目";
    avEl.textContent = "";
    avEl.style.background = "rgba(255,255,255,.18)";
    projectButton.title = "选择一个 Python 项目";
  }
  applyTitleTint(store.currentPath);
}
