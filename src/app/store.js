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

  // 卡片：id -> { el, state, parent, members }
  // 一张卡的四件事放在一条记录里，省得四个平行字典各错各的
  cards: {},

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

// ─── 卡片记录 ───────────────────────────────────────────────────
// el      放置后的 DOM；没放置过是 null
// state   summary | full | mini
// parent  放置时的父卡片 id
// members 类卡片 -> 已出现在目录里的方法（Set，按需生成）

export function card(id) {
  return store.cards[id] || null;
}

/// 取记录，没有就建一条：方法卡在还没放置时常需要先记住目录状态
export function cardRec(id) {
  return store.cards[id] ||
    (store.cards[id] = { el: null, state: "summary", parent: null, members: null });
}

/// 已放置的卡片元素；没放置返回 null
export function placedEl(id) {
  const c = store.cards[id];
  return c && c.el ? c.el : null;
}

export function placedIds(tab) {
  const cards = (tab || store).cards || {};
  return Object.keys(cards).filter(function (id) { return !!cards[id].el; });
}
