// shell/boot.js

// 外壳的公共底座：Tauri 句柄、版本号、写日志、开发期热重载。
// 图本身由 app/ 下的模块负责；这里通过 bootProject() 换项目。

export const api = window.__TAURI__.core;
export const RECENT_KEY = "clew.recentProjects";
export const VERSION = "0.1.0";

// 窗口版没有控制台，JS 异常和取数失败都写进应用数据目录的 clew.log，
// 否则一次「点了没反应」事后只剩重启这一条线索
export function logToFile(text) {
  api.invoke("log_line", { text: String(text) }).catch(function () {});
}
window.__clewLog = logToFile;
window.addEventListener("error", function (e) {
  logToFile("error " + (e.message || "?") + " @ " + (e.filename || "") + ":" + (e.lineno || 0));
});
window.addEventListener("unhandledrejection", function (e) {
  const r = e.reason;
  logToFile("reject " + (r && (r.message || String(r))));
});

// ── 开发期热重载 ────────────────────────────────────────────────
// dev 构建的窗口连的是 tools/dev.ps1 起的本地服务，页面源就是 127.0.0.1。
// 盯着每个前端文件的 Last-Modified，文件一变就整页重载，改完存盘即见。
// 名单由 index.html 后面的 boot.js 给出（它按同一份名单按顺序加载模块），
// 这里不重复列一遍。发版时页面源是 tauri.localhost，热重载不会触发。
(function hotReload() {
  const host = location.hostname;
  if (host !== "127.0.0.1" && host !== "localhost") return;
  const FILES = window.__CLEW_ASSETS__ || [];
  let stamp = null;
  setInterval(function () {
    Promise.all(
      FILES.map(function (f) {
        return fetch(f, { method: "HEAD", cache: "no-store" })
          .then(function (r) {
            // 同一秒里改两次 mtime 分不出来，长度也算一票
            return (r.headers.get("last-modified") || "") + "/" +
              (r.headers.get("content-length") || "");
          })
          .catch(function () {
            return "?";
          });
      })
    )
      .then(function (parts) {
        const now = parts.join(",");
        if (stamp && now !== stamp) location.reload();
        stamp = now;
      })
      .catch(function () {
        /* 服务停了就等它回来 */
      });
  }, 900);
})();

document.addEventListener("keydown", function (e) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r") {
    e.preventDefault();
    location.reload();
  }
});

