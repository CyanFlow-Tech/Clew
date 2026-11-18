// shell/confirm.js
import { store } from "./store.js";
import { api } from "./boot.js";
import { forget } from "./util.js";
import { refreshMissing, renderRecent, setNote, welcome } from "./welcome.js";
import { closeProject } from "./project.js";

// ── 删除项目确认 ────────────────────────────────────────────────
export const confirmLayer = document.createElement("div");
confirmLayer.id = "w-confirm";
confirmLayer.hidden = true;
confirmLayer.innerHTML =
  '<div class="cf-card">' +
  '<h3 id="cf-title"></h3>' +
  '<p id="cf-body"></p>' +
  '<div class="cf-actions">' +
  '<button id="cf-cancel" type="button">取消</button>' +
  '<button id="cf-ok" class="danger" type="button">删除</button>' +
  "</div></div>";

export const cfTitle = confirmLayer.querySelector("#cf-title");
export const cfBody = confirmLayer.querySelector("#cf-body");
export const cfOk = confirmLayer.querySelector("#cf-ok");
export const cfCancel = confirmLayer.querySelector("#cf-cancel");
export let pendingDelete = null;

/// 第一次用到时才挂到欢迎页里：模块装载期读别的模块的节点会在循环依赖里撞 TDZ
function ensureMounted() {
  if (!confirmLayer.isConnected) welcome.appendChild(confirmLayer);
}

export function closeConfirm() {
  confirmLayer.hidden = true;
  pendingDelete = null;
}

export function askDelete(p, missingDir) {
  ensureMounted();
  pendingDelete = p;
  if (missingDir) {
    cfTitle.textContent = "目录不存在";
    cfBody.textContent =
      p.path + "\n\n这个目录已经不在了，建议把这条记录从最近项目里删掉。";
    cfOk.textContent = "删除记录";
  } else {
    cfTitle.textContent = "删除项目「" + p.name + "」";
    cfBody.textContent =
      "会删除 " + p.path + "\\.clew 索引目录，并把它从最近项目里移除。项目源码不受影响。";
    cfOk.textContent = "删除";
  }
  confirmLayer.hidden = false;
}

cfCancel.onclick = closeConfirm;
confirmLayer.onclick = function (e) {
  if (e.target === confirmLayer) closeConfirm();
};
cfOk.onclick = async function () {
  const item = pendingDelete;
  closeConfirm();
  if (!item) return;
  try {
    // 目录已经不在时后端会直接跳过删除，历史记录在这边清掉就行
    await api.invoke("delete_index", { path: item.path });
  } catch (err) {
    setNote(String(err), true);
    return;
  }
  if (store.currentPath === item.path) closeProject();
  forget(item.path);
  renderRecent();
  refreshMissing().then(function () {
    if (!welcome.hidden) renderRecent();
  });
  setNote("「" + item.name + "」已删除，索引一并清除", false);
};

document.addEventListener("keydown", function (e) {
  if (e.key === "Escape" && !confirmLayer.hidden) closeConfirm();
});


// 欢迎页没有项目，就没有菜单可开：左上角只留 logo 和一句欢迎语
export function showWelcome() {
  renderRecent();
  // 目录可能在外面被删掉了，查完再画一遍，给缺目录的行挂上标记
  refreshMissing().then(function () {
    if (!welcome.hidden) renderRecent();
  });
  document.body.classList.add("welcome-open");
  if (store.brandTitle) {
    store.brandTitle.style.display = "";
    store.brandTitle.textContent = "欢迎访问 Clew";
  }
  welcome.hidden = false;
}
export function hideWelcome() {
  document.body.classList.remove("welcome-open");
  if (store.brandTitle) store.brandTitle.style.display = "none";
  welcome.hidden = true;
}


