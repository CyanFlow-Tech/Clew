// shell/welcome.js
import { VERSION, api } from "./boot.js";
import { ICON, avatar, recent, setHidden } from "./util.js";
import { askDelete } from "./confirm.js";
import { chooseFolder, openPath } from "./project.js";
import { settingsPage } from "./settings.js";

// ── 欢迎页 ──────────────────────────────────────────────────────
export const welcome = document.createElement("div");
welcome.id = "welcome";
welcome.innerHTML = `
  <aside id="w-side">
    <div class="w-brand">
      <span class="tb-brand"></span>
      <span><span class="t">Clew</span><br><span class="v">${VERSION}</span>
      <br><span class="auth">qingchu4n</span></span>
    </div>
    <nav class="w-nav">
      <button class="active" data-w-pane="projects">项目</button>
      <button data-w-pane="space">设置</button>
    </nav>
    <div class="w-foot">调用图入口在文件菜单里</div>
  </aside>
  <main id="w-main">
    <div id="w-projects">
      <div class="w-toolbar">
        <label class="w-search">
          <input id="w-query" spellcheck="false" placeholder="搜索项目">
        </label>
        <div class="w-actions"><button id="w-open">打开新项目</button></div>
      </div>
      <div id="w-note" hidden></div>
      <div id="w-list">
        <section class="w-sec">
          <div class="w-sec-h">最近项目</div>
          <ul id="w-recent"></ul>
        </section>
        <section class="w-sec" id="w-hidden-sec" hidden>
          <button class="w-sec-h toggle" id="w-hidden-toggle" type="button">
            <span class="chev">${ICON.chevronRight}</span>
            <span>隐藏项目</span>
            <span class="cnt" id="w-hidden-cnt"></span>
          </button>
          <ul id="w-hidden" hidden></ul>
        </section>
      </div>
    </div>
    <div id="w-space" hidden></div>
  </main>`;
// 先藏着：启动时可能直接打开上次的项目，不能先闪一下欢迎页
welcome.hidden = true;
document.body.appendChild(welcome);
// 侧栏用同一张位图的放大版，标题栏是 20px，这里 36px 由 CSS 接管
welcome.querySelector(".w-brand .tb-brand").innerHTML =
  '<img src="logo.png" alt="" width="36" height="36">';

export const recentList = welcome.querySelector("#w-recent");
export const hiddenSec = welcome.querySelector("#w-hidden-sec");
export const hiddenList = welcome.querySelector("#w-hidden");
export const hiddenToggle = welcome.querySelector("#w-hidden-toggle");
export const hiddenCnt = welcome.querySelector("#w-hidden-cnt");
export const queryInput = welcome.querySelector("#w-query");
export const noteEl = welcome.querySelector("#w-note");

// 隐藏区默认折叠，展开状态记在一次会话里，不写进本地存储
export let hiddenOpen = false;
hiddenToggle.onclick = function () {
  hiddenOpen = !hiddenOpen;
  hiddenToggle.classList.toggle("open", hiddenOpen);
  hiddenList.hidden = !hiddenOpen;
};

// 欢迎页自己的提示条：删除结果、打开失败的原因都落在这里，不再只写进被盖住的顶栏
export function setNote(text, isError) {
  noteEl.textContent = text || "";
  noteEl.hidden = !text;
  noteEl.classList.toggle("err", !!isError);
}

// 目录存在性：进欢迎页时批量查一次，删掉记录后再查一次
export let missing = {};

export async function refreshMissing() {
  const list = recent();
  if (!list.length) {
    missing = {};
    return;
  }
  try {
    const flags = await api.invoke("check_paths", { paths: list.map((p) => p.path) });
    const next = {};
    list.forEach((p, i) => {
      if (!flags[i]) next[p.path] = true;
    });
    missing = next;
  } catch (err) {
    missing = {};
  }
}

// 一行项目：头像、名字路径、右侧两个动作。动作区自己吃掉点击，
// 免得点眼睛或垃圾桶的时候把项目打开。
export function welcomeRow(p) {
  const li = document.createElement("li");
  li.appendChild(avatar(p.name, "w-avatar"));

  const meta = document.createElement("div");
  meta.className = "w-meta";
  const nameline = document.createElement("div");
  nameline.className = "w-nameline";
  const name = document.createElement("div");
  name.className = "w-name";
  name.textContent = p.name;
  nameline.appendChild(name);

  // 目录被外部删掉的记录，名字后面挂一个感叹号，鼠标停上去说明原因
  const gone = !!missing[p.path];
  if (gone) {
    li.classList.add("gone");
    const badge = document.createElement("span");
    badge.className = "w-missing";
    badge.title = "路径不存在：" + p.path;
    badge.innerHTML = ICON.warn;
    nameline.appendChild(badge);
  }

  const path = document.createElement("div");
  path.className = "w-path";
  path.textContent = p.path;
  meta.appendChild(nameline);
  meta.appendChild(path);
  li.appendChild(meta);

  const acts = document.createElement("div");
  acts.className = "w-acts";
  acts.onclick = (e) => e.stopPropagation();

  const eye = document.createElement("button");
  eye.type = "button";
  eye.className = "w-act";
  eye.title = p.hidden ? "恢复显示" : "隐藏";
  eye.innerHTML = p.hidden ? ICON.eye : ICON.eyeOff;
  eye.onclick = function (e) {
    e.stopPropagation();
    setHidden(p.path, !p.hidden);
    renderRecent();
    setNote(
      p.hidden ? p.name + " 已恢复显示" : p.name + " 已隐藏，可在下方隐藏项目里恢复",
      false,
    );
  };
  acts.appendChild(eye);

  const del = document.createElement("button");
  del.type = "button";
  del.className = "w-act danger";
  del.title = "删除项目";
  del.innerHTML = ICON.trash;
  del.onclick = function (e) {
    e.stopPropagation();
    askDelete(p);
  };
  acts.appendChild(del);
  li.appendChild(acts);

  // 目录不存在时点击只给清理入口；隐藏的项目先要求恢复可见
  if (gone) {
    li.onclick = () => askDelete(p, true);
  } else if (p.hidden) {
    li.onclick = () => setNote("「" + p.name + "」已隐藏，先点眼睛图标恢复显示再打开", true);
  } else {
    li.onclick = () => openPath(p.path);
  }
  return li;
}

export function hintRow(text) {
  const li = document.createElement("li");
  li.className = "w-hint";
  li.style.cursor = "default";
  li.textContent = text;
  return li;
}

export function renderRecent() {
  const q = queryInput.value.trim().toLowerCase();
  const match = (p) =>
    !q || p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q);
  const all = recent();
  const visible = all.filter((p) => !p.hidden && match(p));
  const hidden = all.filter((p) => p.hidden && match(p));

  recentList.innerHTML = "";
  if (!visible.length) {
    recentList.appendChild(
      hintRow(
        q
          ? "没有匹配的项目"
          : "还没有项目。点右上角「打开新项目」，选一个 Python 目录（首次打开会自动建索引）。",
      ),
    );
  }
  for (const p of visible) recentList.appendChild(welcomeRow(p));

  hiddenSec.hidden = hidden.length === 0;
  hiddenCnt.textContent = hidden.length ? "（" + hidden.length + "）" : "";
  hiddenList.innerHTML = "";
  for (const p of hidden) hiddenList.appendChild(welcomeRow(p));
  if (!hidden.length) hiddenOpen = false;
  hiddenList.hidden = !hiddenOpen;
  hiddenToggle.classList.toggle("open", hiddenOpen);
}

// 欢迎页自己的两个入口：搜索框过滤最近项目，打开按钮走项目的选择文件对话框
queryInput.oninput = renderRecent;
welcome.querySelector("#w-open").onclick = () => chooseFolder();

// 欢迎页：同一个设置页，只放这一页，不铺别的设置
export const wProjects = welcome.querySelector("#w-projects");
export const wSpace = welcome.querySelector("#w-space");
export const welcomePage = settingsPage();
wSpace.appendChild(welcomePage.el);

export function showWelcomePane(name) {
  const onSpace = name === "space";
  wProjects.hidden = onSpace;
  wSpace.hidden = !onSpace;
  for (const btn of welcome.querySelectorAll(".w-nav button")) {
    btn.classList.toggle("active", btn.dataset.wPane === name);
  }
  if (onSpace) welcomePage.refresh();
}

for (const btn of welcome.querySelectorAll(".w-nav button")) {
  btn.onclick = function () {
    showWelcomePane(btn.dataset.wPane);
  };
}


