// shell/favorites.js
import { store } from "./store.js";
import { STAR_SVG } from "../shared/icons.js";
import { api } from "./boot.js";
import { setNote } from "./welcome.js";
import { spRender } from "./siderows.js";

// ── 收藏状态 ────────────────────────────────────────────────────
// 单一真源 + 订阅：谁要显示收藏状态就读 favorites.has(id)，收藏变了
// 只广播「哪个 id 变了」，各视图自己决定怎么最小更新，互不认识。
// 早先是每个视图各自去 rootData.favorites 里 indexOf，加一个视图就得
// 记得去补一处通知，漏掉就不同步。
export const favorites = {
  set: new Set(),
  subs: [],
  sync(list) {
    favorites.set = new Set(list || []);
  },
  has(id) {
    return !!id && favorites.set.has(id);
  },
  subscribe(fn) {
    favorites.subs.push(fn);
  },
  emit(id) {
    favorites.subs.forEach(function (fn) {
      try {
        fn(id);
      } catch (err) {
        if (window.__clewLog) window.__clewLog("favorites 订阅者出错：" + err);
      }
    });
  },
};

export async function toggleFavorite(id) {
  if (!store.currentPath) return;
  try {
    store.rootData = await api.invoke("roots", {
      project: store.currentPath,
      action: "toggle",
      cardId: id,
    });
  } catch (err) {
    return;
  }
  favorites.sync(store.rootData.favorites);
  favorites.emit(id);
}

// 卡片头上的旗标按钮走这两个口
window.__clewIsFavorite = function (id) {
  return favorites.has(id);
};
window.__clewToggleFavorite = function (id) {
  return toggleFavorite(id);
};
// 画布取不到卡片时用它说话，否则一次失败只剩一片空白，看不出发生了什么
window.__clewNote = function (text, isError) {
  setNote(text, !!isError);
};


// 画布在切根和打开根之后回调这里，顶栏按钮跟着变

// 打开不算「用过」：一口气点开十几个函数全记进去，就没意义了。
// 只有用户在卡片上真的点过东西，画布才会喊这里（见 __clewCardUsed）。
window.__clewRootOpened = function (rootId) {
  if (!store.currentPath || !rootId) return;
  // 侧栏在文件末尾才挂上这个回调，早于此的调用直接跳过
  if (window.__clewRootChanged) window.__clewRootChanged(rootId);
};

window.__clewCardUsed = function (id) {
  if (!store.currentPath || !id) return;
  api.invoke("roots", { project: store.currentPath, action: "recent", cardId: id })
    .then(function (data) {
      store.rootData = data;
      if (store.spView === "recent") spRender();
    })
    .catch(function () {});
};


// 启动时先把收藏读出来，卡片头上的旗标才有正确的初始状态


