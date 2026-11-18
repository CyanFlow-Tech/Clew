// 引导：向后端要启动信息，再按顺序装载 app/ 与 shell/，都在了才让窗口显示。
// 全程走 Tauri：没有浏览器预览这条路。

const api = window.__TAURI__.core;

// 启动耗时打点。统一收集在 __CLEW_T，最后一次性打到 console。
const __CLEW_T = [];
function mark(name) {
  __CLEW_T.push([name, performance.now()]);
  return performance.now();
}
mark("script");
window.__CLEW_MARK = mark;

function clewReport() {
  const first = __CLEW_T[0][1];
  const parts = __CLEW_T.map(function (p) {
    return p[0] + "=" + (p[1] - first).toFixed(1);
  });
  const gaps = [];
  for (let i = 1; i < __CLEW_T.length; i++) {
    gaps.push(
      __CLEW_T[i][0] + ":" + (__CLEW_T[i][1] - __CLEW_T[i - 1][1]).toFixed(1)
    );
  }
  console.log("[t] JS " + parts.join(" ") + " | gaps " + gaps.join(" "));
}
window.__CLEW_REPORT = clewReport;

function clewFail(msg) {
  document.body.innerHTML =
    '<pre style="color:#f88;padding:24px;white-space:pre-wrap">' + msg + "</pre>";
  // 报错时窗口可能还藏着，先亮出来，不然用户看到的是什么也没发生
  api.invoke("ready").catch(function () {});
}

window.onerror = function (msg, src, line, col, err) {
  clewFail(msg + "\n" + (src || "") + ":" + line + ":" + col +
           "\n" + ((err && err.stack) || ""));
};
window.addEventListener("unhandledrejection", function (ev) {
  clewFail("promise: " + String((ev.reason && ev.reason.stack) || ev.reason));
});

// 开发期热重载盯的就是这一份：改了任何一个都要整页重载
window.__CLEW_ASSETS__ = [
  "index.html",
  "boot.js",
  "themes.js",
  "css/base.css", "css/shell.css", "css/sidebar.css", "css/settings.css",
  "vendor/icons.js", "vendor/icons-antd.js", "vendor/icons-material.js",
  "shared/format.js", "shared/icons.js",
  "app/index.js", "app/env.js", "app/dom.js", "app/store.js", "app/data.js",
  "app/format.js", "app/highlight.js", "app/card.js", "app/place.js",
  "app/layout.js", "app/edge.js", "app/select.js", "app/workspace.js",
  "app/loadtab.js", "app/canvas.js",
  "shell/index.js", "shell/boot.js", "shell/util.js", "shell/id.js",
  "shell/welcome.js", "shell/confirm.js", "shell/titlebar.js",
  "shell/projectpanel.js", "shell/filemenu.js", "shell/help.js",
  "shell/project.js", "shell/scan.js", "shell/startup.js", "shell/favorites.js",
  "shell/sidebar.js", "shell/settings.js", "shell/settings-block.js", "shell/sidebase.js",
  "shell/sidesearch.js", "shell/resizer.js", "shell/viewmenu.js",
  "shell/dirtree.js", "shell/siderows.js", "shell/sidetoggle.js",
  "shell/sidetab.js",
];

// 开发期给模块 URL 加版本串：WebView 会缓存，改了刷新还是旧内容。
// 静态 import 带不了参数，所以入口模块用动态 import 加载。
function versioned(path) {
  const dev =
    location.hostname === "127.0.0.1" || location.hostname === "localhost";
  return dev ? path + "?v=" + Date.now() : path;
}

async function clewBoot() {
  mark("boot-call");
  const info = await api.invoke("boot");
  mark("boot-done");
  window.__CLEW_BOOT_MS = info && info.boot_ms;
  window.__CLEW_BOOT_INFO__ = info;
  if (info) window.__CLEW_PATH__ = info.root;
  return info;
}

async function clewMount(info) {
  window.__CLEW_ENTRIES__ = (info && info.entries) || [];
  await import(versioned("./app/index.js"));
  mark("app-loaded");
  await import(versioned("./shell/index.js"));
  mark("shell-loaded");
  // 外壳的 DOM 已经就位，直接让窗口出现。页面加载完成那边还有一层，
  // 所以这里不猜时间，也不需要等渲染帧。
  api.invoke("ready").catch(function () {});
}

try {
  await clewMount(await clewBoot());
} catch (err) {
  clewFail(String((err && err.stack) || err));
}
