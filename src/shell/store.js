// shell/store.js
// 外壳共享的可变状态。模块之间只通过这里交换，谁都不许各自留一份副本。

/// Windows canonicalize 的 \\?\ 前缀、早期版本写进 localStorage 的重复反斜杠
function normPath(p) {
  return String(p || "")
    .replace(/^\\\\\?\\/, "")
    .replace(/\\\\+/g, "\\");
}

export const store = {
  // 当前项目
  currentPath: window.__CLEW_PATH__ ? normPath(window.__CLEW_PATH__) : null,
  rootData: { favorites: [], recent: [] },   // 收藏与最近打开的根
  brandTitle: null,                          // 顶栏标题节点，开到哪个项目就改它

  // 侧栏
  sideOpen: false,
  spView: "all",
  spFiles: null,        // [{module, path, is_third_party, has_main_guard, symbols}]
  spFileByModule: new Map(),
  spEntrySyms: null,    // 入口视图：全项目入口定义，通常几十条
  spLoadedFor: null,    // 这份数据属于哪个项目
  spCache: new Map(),   // module -> 该文件的定义
  spTree: null,         // 当前视图的目录树，展开时就地取子节点
  spSeq: 0,             // 渲染令牌，连点展开时只让最后一次往上画
};

