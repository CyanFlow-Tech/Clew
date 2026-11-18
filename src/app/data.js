// app/data.js
// 按需取卡片：请求合并、超时、被调次数合并。数据本身在 store 里。
import { store } from "./store.js";
import { desktopApi, CARD_TIMEOUT } from "./env.js";

const inflight = {};

function withTimeout(p, ms, label) {
  return new Promise(function (resolve, reject) {
    let settled = false;
    const timer = setTimeout(function () {
      if (settled) return;
      settled = true;
      reject(new Error(label + " 超时 " + ms + "ms"));
    }, ms);
    p.then(
      function (v) { if (!settled) { settled = true; clearTimeout(timer); resolve(v); } },
      function (e) { if (!settled) { settled = true; clearTimeout(timer); reject(e); } }
    );
  });
}

/// 后端随每批卡片带回全项目的被调次数，比在已加载的子图里数更准
export function mergeFanin(map) {
  Object.keys(map || {}).forEach(function (k) { store.FANIN[k] = map[k]; });
}

/// 保证这些 id 的卡片已在 store 里。同一张卡同时点两次只发一次请求。
export function ensure(ids) {
  const want = [];
  const waits = [];
  ids.forEach(function (id) {
    if (!id || store.NODES[id]) return;
    if (inflight[id]) { waits.push(inflight[id]); return; }
    if (want.indexOf(id) === -1) want.push(id);
  });
  if (want.length) {
    const p = withTimeout(desktopApi.invoke("cards", { ids: want }), CARD_TIMEOUT, "cards")
      .then(function (payload) {
        const cards = payload.cards || {};
        const calls = payload.calls || {};
        const refs = payload.refs || {};
        Object.keys(cards).forEach(function (k) { store.NODES[k] = cards[k]; });
        Object.keys(calls).forEach(function (k) { store.CALLS[k] = calls[k]; });
        Object.keys(refs).forEach(function (k) { store.REFS[k] = refs[k]; });
        mergeFanin(payload.fanin);
      });
    want.forEach(function (id) { inflight[id] = p; });
    p.catch(function (err) {
      console.error("取卡片失败", err);
      if (window.__clewLog) window.__clewLog("cards " + want.join(",") + " 失败：" + err);
    }).then(function () {
      want.forEach(function (id) { delete inflight[id]; });
    });
    waits.push(p);
  }
  return Promise.all(waits);
}
