// shell/settings-block.js
import { h } from "../shared/dom.js";
// 设置页的一块：标题 + 一行（路径框 + 迁移）。两块只有「搬什么」不同。
import { api } from "./boot.js";
import { normPath } from "./util.js";

export const spacePanes = [];

// ── 设置 ────────────────────────────────────────────────────────
// 齿轮钉在活动栏最下面，点开一个弹窗：左边导航，右边内容。
// 欢迎页那边只放「项目空间」这一项，两边共用同一块内容组件，设置项不写两遍。
// 第一版只有项目空间一项；以后加项就在这里加导航按钮与对应面板。
export const ST_TEXT = {
  title: "设置",
  nav: "目录",
  data: "用户数据目录",
  cache: "界面缓存目录",
  pathLabel: "路径",
  toDefault: "默认",
  migrate: "迁移",
  migrating: "正在迁移",
  restart: "重启后生效",
};
/// 设置页：两个目录，各一块，用标题区分。两处（弹窗与欢迎页）共用同一个构造函数，
/// 每个容器一份实例，打开时重读一次状态。

/// `notify(kind)` 由设置页给，用来把别的容器（欢迎页那份）的同一块刷新过来。
export function directoryBlock(kind, notify) {
  const isCache = kind === "cache";
  const box = h("div", { class: "st-pane", dataset: { block: kind } });
  const title = isCache ? ST_TEXT.cache : ST_TEXT.data;
  box.innerHTML = `
    <div class="st-h">${title}</div>
    <div class="st-field">
      <span class="st-k">${ST_TEXT.pathLabel}</span>
      <input class="st-input" data-k="path" readonly spellcheck="false">
      <button type="button" class="st-btn" data-k="default" disabled>${ST_TEXT.toDefault}</button>
      <button type="button" class="st-btn" data-k="migrate" disabled>${ST_TEXT.migrate}</button>
    </div>
    <div class="st-note" data-k="note" hidden></div>`;

  const pathEl = box.querySelector('[data-k="path"]');
  const defaultBtn = box.querySelector('[data-k="default"]');
  const migrateBtn = box.querySelector('[data-k="migrate"]');
  const noteEl = box.querySelector('[data-k="note"]');

  // 一行只有一个路径：框里放的就是「要用哪个目录」。
  // 两块都要点迁移才搬，语义一致；区别只在搬的内容与生效时机。
  let home = "";
  let picked = "";
  let defaultPath = "";
  let busy = false;

  function setNote(text, isError) {
    noteEl.textContent = text || "";
    noteEl.hidden = !text;
    noteEl.classList.toggle("err", !!isError);
  }

  function render() {
    pathEl.value = picked || home;
    pathEl.title = pathEl.value;
    migrateBtn.disabled = busy || !picked || picked === home;
    migrateBtn.textContent = busy ? ST_TEXT.migrating : ST_TEXT.migrate;
    defaultBtn.disabled = busy || !home || home === defaultPath;
  }

  async function refresh() {
    setNote("");
    try {
      const state = await api.invoke("space_state");
      home = normPath(isCache ? state.webview_dir : state.home);
      defaultPath = normPath(isCache ? state.webview_default : state.default_home);
      picked = home;
    } catch (err) {
      setNote(String(err), true);
    }
    render();
  }

  // 点框只选目录，不写任何设置：真正搬动要按迁移
  async function pick() {
    if (busy) return;
    let dir = null;
    try {
      dir = await window.__TAURI__.dialog.open({ directory: true, multiple: false });
    } catch (err) {
      setNote(String(err), true);
      return;
    }
    if (!dir) return;
    picked = normPath(dir);
    setNote("");
    render();
  }

  pathEl.onclick = pick;

  // 「默认」只把默认值填进框里，不写设置也不搬东西——和点框选目录同构，
  // 真正搬动统一由「迁移」执行。
  defaultBtn.onclick = function () {
    if (busy || !defaultPath) return;
    picked = defaultPath;
    setNote("");
    render();
  };

  migrateBtn.onclick = async function () {
    if (!picked || busy || picked === home) return;
    busy = true;
    setNote("");
    render();
    try {
      const target = picked;
      if (isCache) {
        home = normPath(await api.invoke("migrate_webview_dir", { target: target }));
        // WebView2 在进程起来时就把目录锁死了，换目录只能下次启动生效
        setNote(ST_TEXT.restart, false);
      } else {
        const report = await api.invoke("migrate_space", { target: target });
        home = normPath(report.home);
      }
      picked = home;
      // 别的容器（欢迎页那份）同一块也要跟着换过来
      notify(kind);
    } catch (err) {
      setNote(String(err), true);
    } finally {
      busy = false;
      render();
    }
  };

  render();
  return {
    el: box,
    refresh: refresh,
  };
}
