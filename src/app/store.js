// app/store.js
// 画布的数据与界面状态。所有模块共享这一份：改属性，不要换对象。
// 拆分前它散在 app.js 的闭包里，靠 getter/setter 让别的模块看到重新赋值；
// 现在是模块导出的一个对象，谁改谁读都是同一份。

export const store = {
  // 数据：按展开步取，ENTRIES 由引导脚本从 boot() 的结果塞进来
  NODES: {},
  CALLS: {},
  ENTRIES: window.__CLEW_ENTRIES__ || [],
  FANIN: {},
  REFS: {},          // 未加载的卡也知道 module / qualname，候选列表才显示得出名字

  // 卡片
  cardEls: {},       // 已放置的卡片 DOM
  state: {},         // summary | full | mini
  openMethods: {},   // 类卡片里展开的方法
  parentOf: {},      // 放置时的父卡片
  methodSet: {},     // 类卡片 -> 已出现的成员方法（按需生成）
  activated: new Set(),   // 被用户实际跟进过的调用点（cs id）
  selectedCs: null,       // 当前显示绑定注释的那一跳
  selectedRow: null,      // 当前选中的方法（用于类目录高亮）
  selectedId: null,       // 当前选中的卡片
  roots: new Set(),       // 入口卡片

  // 每个根一份独立的图
  tabs: new Map(),        // 根 card id -> 这份图的状态
  activeRoot: null,
  trail: [],              // 顶部路径条
};
