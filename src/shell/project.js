// shell/project.js
import { store } from "./store.js";
import { api } from "./boot.js";
import { basename, normPath, remember, setHidden } from "./util.js";
import { setNote } from "./welcome.js";
import { hideWelcome, showWelcome } from "./confirm.js";
import { setStatus } from "./titlebar.js";
import { renderProjectButton } from "./projectpanel.js";
import { setMenuMode } from "./filemenu.js";
import { favorites } from "./favorites.js";
import { spRender } from "./siderows.js";
import { setSideOpen } from "./sidetoggle.js";
import { spSyncProject } from "./sidetab.js";

// ── 项目操作 ────────────────────────────────────────────────────
// 收藏与最近，来自 roots.json；侧栏的收藏夹 / 最近使用两个视图都读它

import { stopScanWatch, watchScan } from "./scan.js";


// 上次开着的项目：没主动关闭的话，下次启动直接打开它
export const LAST_KEY = "clew.lastProject";

/// 读这个项目的收藏与最近使用。命令行启动不经过 enterProject、
/// 自动重开也不经过启动块，所以两处都要能调。
export function loadRoots() {
  if (!store.currentPath) return;
  api.invoke("roots", { project: store.currentPath, action: "get" })
    .then(function (d) {
      store.rootData = d || { favorites: [], recent: [] };
      favorites.sync(store.rootData.favorites);
      if (store.sideOpen) spRender();
    })
    .catch(function () {});
}

export function enterProject(path) {
  store.currentPath = normPath(path);
  // 画布的工作区快照按项目分开存，app 那边用这个 key
  window.__CLEW_PROJECT__ = store.currentPath;
  remember(path);
  try {
    localStorage.setItem(LAST_KEY, store.currentPath);
  } catch (err) {
    /* 隐私模式写不进就算了 */
  }
  loadRoots();
  // 主动打开的项目不该继续留在隐藏区
  setHidden(path, false);
  renderProjectButton();
  hideWelcome();
  setNote("");
}

export async function openPath(path) {
  stopScanWatch();
  setStatus("正在打开…");
  try {
    const info = await api.invoke("open_project", { path: path });
    // 首次打开：先进项目，索引在后台建，进度画在状态栏
    if (info && info.state === "indexing") {
      enterProject(path);
      watchScan(path);
      return;
    }
    setStatus("");
    enterProject(path);
    await bootProject(info && info.entries);
    setSideOpen(true);
  } catch (err) {
    setStatus(String(err), true);
    setNote(String(err), true);
    if (typeof tabs !== "undefined" && tabs.size === 0) showWelcome();
  }
}

// 重建索引：删掉旧库再走一次第一次打开的流程，后台扫描照旧走进度条
export async function rebuildIndex() {
  if (!store.currentPath) return;
  if (scanTimer) {
    setStatus("索引还在建立，等它完成再重建", true);
    return;
  }
  const path = store.currentPath;
  try {
    await api.invoke("delete_index", { path: path });
  } catch (err) {
    setStatus(String(err), true);
    return;
  }
  bootProject([]);
  setMenuMode(false);
  await openPath(path);
}

// 删除索引：库没了就不能留在项目里，回到欢迎页等下次重建
export async function removeIndex() {
  if (!store.currentPath) return;
  if (scanTimer) {
    setStatus("索引还在建立，等它完成再删除", true);
    return;
  }
  const path = store.currentPath;
  const name = basename(path);
  try {
    await api.invoke("delete_index", { path: path });
  } catch (err) {
    setStatus(String(err), true);
    return;
  }
  setMenuMode(false);
  closeProject();
  setNote("「" + name + "」的索引已删除，下次打开会重新建立", false);
}

export async function chooseFolder() {
  let dir = null;
  try {
    dir = await window.__TAURI__.dialog.open({
      directory: true,
      multiple: false,
      title: "选择要分析的 Python 项目",
    });
  } catch (err) {
    setStatus(String(err), true);
    setNote(String(err), true);
    return;
  }
  if (!dir) return;
  await openPath(dir);
}

export function closeProject() {
  stopScanWatch();
  setMenuMode(false);
  bootProject([]);
  store.currentPath = null;
  window.__CLEW_PROJECT__ = null;
  try {
    // 主动关掉的项目，下次不再自动打开
    localStorage.removeItem(LAST_KEY);
  } catch (err) {
    /* 读不到 localStorage 就算了 */
  }
  renderProjectButton();
  setStatus("");
  showWelcome();
  spSyncProject();
}
