// shell/startup.js
import { store } from "./store.js";
import { api } from "./boot.js";
import { splitId } from "./id.js";
import { remember } from "./util.js";
import { icon } from "../shared/icons.js";
import { hideWelcome, showWelcome } from "./confirm.js";
import { renderProjectButton } from "./projectpanel.js";
import { LAST_KEY, loadRoots, openPath } from "./project.js";
import { watchScan } from "./scan.js";

// ── 启动 ────────────────────────────────────────────────────────
if (store.currentPath) remember(store.currentPath);
renderProjectButton();

// 启动顺序：命令行带的项目 > 上次没关掉的项目 > 欢迎页
(async function startup() {
  const bootInfo = window.__CLEW_BOOT_INFO__;
  // 命令行启动不经过 enterProject，这里补一次，否则下次不带参数就进不去
  if (store.currentPath) {
    try {
      localStorage.setItem(LAST_KEY, store.currentPath);
    } catch (err) {
      /* 写不进就算了 */
    }
  }
  if (bootInfo && bootInfo.state === "indexing") {
    hideWelcome();
    watchScan(bootInfo.root);
    return;
  }
  // 命令行带了项目、或者上次开着的项目还活着，就直接进画布
  if (store.currentPath && (window.__CLEW_ENTRIES__ || []).length) {
    hideWelcome();
    return;
  }
  if (!store.currentPath) {
    let last = null;
    try {
      last = localStorage.getItem(LAST_KEY);
    } catch (err) {
      /* 读不到就当没有 */
    }
    if (last) {
      // 目录可能已经被删掉了，先问一句再决定进不进
      let alive = false;
      try {
        const rows = await api.invoke("check_paths", { paths: [last] });
        alive = !!(rows && rows[0]);
      } catch (err) {
        alive = false;
      }
      if (alive) {
        await openPath(last);
        return;
      }
      try {
        localStorage.removeItem(LAST_KEY);
      } catch (err) {
        /* 无所谓的清理 */
      }
    }
  }
  showWelcome();
})();


loadRoots();



