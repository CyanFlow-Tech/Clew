
let DATA = window.__CLEW__ || {};
let NODES = DATA.cards || {}, CALLS = DATA.calls || {}, ENTRIES = DATA.entries || window.__CLEW_ENTRIES__ || [];
const SUMMARY = 6, CARD_W = 720, COL_GAP = 70, ROW_GAP = 34, HEAD_H = 26;

// 桌面模式下数据按需取：点开一个节点才向后端要它的卡片与出边。
// 浏览器预览读的是一份完整 JSON，这时 LAZY 为假，行为与以前一致。
const desktopApi = window.__TAURI__ ? window.__TAURI__.core : null;
const LAZY = !window.__CLEW__ && !!desktopApi;

const FANIN = {};
const REFS = {};   // 未加载的卡也知道 module / qualname，候选列表才显示得出名字
function countFanin() {
  Object.keys(FANIN).forEach(function (k) { delete FANIN[k]; });
  Object.keys(CALLS).forEach(function (k) {
    const c = CALLS[k];
    if (c.to) FANIN[c.to] = (FANIN[c.to] || 0) + 1;
  });
}

// 后端随每批卡片带回全项目的被调次数，比在已加载的子图里数更准
function mergeFanin(map) {
  Object.keys(map || {}).forEach(function (k) { FANIN[k] = map[k]; });
}

const inflight = {};
function ensure(ids) {
  if (!LAZY) return Promise.resolve();
  const want = [];
  const waits = [];
  ids.forEach(function (id) {
    if (!id || NODES[id]) return;
    if (inflight[id]) { waits.push(inflight[id]); return; }
    if (want.indexOf(id) === -1) want.push(id);
  });
  if (want.length) {
    const p = desktopApi.invoke("cards", { ids: want }).then(function (payload) {
      const cards = payload.cards || {};
      const calls = payload.calls || {};
      const refs = payload.refs || {};
      Object.keys(cards).forEach(function (k) { NODES[k] = cards[k]; });
      Object.keys(calls).forEach(function (k) { CALLS[k] = calls[k]; });
      Object.keys(refs).forEach(function (k) { REFS[k] = refs[k]; });
      mergeFanin(payload.fanin);
    });
    want.forEach(function (id) { inflight[id] = p; });
    p.catch(function (err) {
      console.error("取卡片失败", err);
    }).then(function () {
      want.forEach(function (id) { delete inflight[id]; });
    });
    waits.push(p);
  }
  return Promise.all(waits);
}

const world = document.getElementById("world");
const svg = document.getElementById("edges");
const graph = document.getElementById("graph");
const bubble = document.getElementById("bubble");
const focusLayer = document.getElementById("focus");

let cardEls = {};         // 已放置的卡片 DOM
let state = {};           // summary | full | mini
let openMethods = {};     // 类卡片里展开的方法
let parentOf = {};        // 放置时的父卡片
let methodSet = {};       // 类卡片 -> 已出现的成员方法（按需生成）
let activated = new Set();   // 被用户实际跟进过的调用点（cs id）
let selectedCs = null;       // 当前显示绑定注释的那一跳
let selectedRow = null;      // 当前选中的方法（用于类目录高亮）
let selectedId = null;       // 当前选中的卡片
let roots = new Set();    // 入口卡片

function esc(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
function hasMore(id) { return NODES[id].lines.length > SUMMARY; }
function toggleLabel(id, st) {
  if (st === "mini") return "展开";
  if (st === "full") return "折叠";
  return hasMore(id) ? "全文" : "折叠";
}
function nextState(id, st) {
  if (st === "mini") return "summary";
  if (st === "full") return "mini";
  return hasMore(id) ? "full" : "mini";
}

// ─── 卡片渲染 ───────────────────────────────────────────────────
function shortText(s, n) { return s.length > n ? s.slice(0, n) + "…" : s; }



function renderLine(ln, host, ownerId) {
  const row = document.createElement("div");
  row.className = "ln " + ln.k;
  row.style.paddingLeft = (34 + (ln.indent || 0) * 12) + "px";
  let html = esc(ln.text);
  const c = ln.cs ? CALLS[ln.cs] : null;
  if (c && c.imported && c.target) {
    const re = new RegExp("\\b" + c.target.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "\\b");
    if (re.test(html)) html = html.replace(re, '<span class="ext" title="' + esc(c.imported) + '">$&</span>');
  }
  row.innerHTML = html;
  if (ln.ln) {
    const num = document.createElement("span");
    num.className = "lnum";
    num.textContent = ln.ln;
    row.appendChild(num);
  }
  if (c && c.to) {
    const a = document.createElement("span");
    let cls = "anchor";
    if (c.conf === "heur") cls += " heur";
    else if (c.conf === "poly") cls += " poly";
    else if (cardEls[c.to]) cls += " done";
    a.className = cls;
    a.dataset.cs = ln.cs;
    const cnt = c.candidates && c.candidates.length > 1
      ? (c.candidates.length > 9 ? "+" : c.candidates.length) : "";
    a.textContent = "◆" + cnt;
    if (selectedCs === ln.cs) a.className += " on";
    a.title = "展开并显示这一跳的实参绑定";
    a.onmouseenter = function (ev) {
      showBubble(ln.cs, ev);
      if (c.to && !NODES[c.to]) {
        // 形参在目标卡里，取回来再把气泡画一遍
        ensure([c.to]).then(function () {
          if (hoverCs === ln.cs) showBubble(ln.cs, ev);
        });
      }
    };
    a.onmouseleave = hideBubble;
    a.onclick = function (ev) {
      ev.stopPropagation();
      hideBubble();
      selectedCs = ln.cs;
      ensure(c.to ? [c.to] : []).then(function () {
        if (ownerId && cardEls[ownerId]) { renderCard(ownerId); relayout(); }
        expand(c.to, c.from, ln.cs);
      });
    };
    row.appendChild(a);
  } else if (c) {
    const a = document.createElement("span");
    a.className = "anchor dead";
    a.textContent = "◆";
    a.title = "未解析（" + (c.conf === "builtin" ? "内置/库函数" : "找不到定义") + "）";
    row.appendChild(a);
  }
  host.appendChild(row);

  // 被选中的调用：下一行插入实参绑定注释
  if (c && selectedCs === ln.cs) {
    const box = document.createElement("div");
    box.className = "bind";
    box.textContent = bindingText(c);
    host.appendChild(box);
  }
}

// 完整绑定文本：所有能对上的形参，换行不了的自动折行
function bindingText(c) {
  const to = NODES[c.to];
  if (!to) return "";
  const params = to.params || [];
  if (!params.length) return "（无参数）";
  const parts = params.map(function (p) {
    const v = (c.args || {})[p];
    if (v === undefined) return p + " ← ?";
    return p + " ← " + v;
  });
  return parts.join("      ");
}

function renderCard(id) {
  const n = NODES[id], st = state[id] || "summary", el = cardEls[id];
  el.className = "card" + (st === "mini" ? " mini" : "");
  el.innerHTML = "";

  const head = document.createElement("div");
  head.className = "head";
  head.innerHTML = '<span class="t">' + esc(n.title) + '</span>' +
    (FANIN[id] > 1 ? '<span class="hub">被调 ' + FANIN[id] + '</span>' : '') +
    (n.kind === "class" ? '<span class="kindbadge">类</span>' : '') +
    (n.kind === "class" && n.bases && n.bases.length ?
      '<span class="kindbadge">← ' + esc(n.bases.join(", ")) + '</span>' : '') +
    '<span class="m">' + esc(n.module) + '</span>' +
    '<span class="btn tg">' + toggleLabel(id, st) + '</span>' +
    '<span class="btn pin' +
      (window.__clewIsFavorite && window.__clewIsFavorite(id) ? " on" : "") +
      '" title="标记为入口">⚑</span>' +
    (roots.has(id) ? '' : '<span class="btn close">✕</span>');
  head.onclick = function () { select(id); };
  head.ondblclick = function (e) { e.stopPropagation(); openFocus(id); };
  head.querySelector(".tg").onclick = function (e) {
    e.stopPropagation();
    state[id] = nextState(id, st);
    renderCard(id);
    relayout();
  };
  const closeBtn = head.querySelector(".close");
  if (closeBtn) closeBtn.onclick = function (e) {
    e.stopPropagation();
    closeCard(id);
  };
  const pinBtn = head.querySelector(".pin");
  if (pinBtn) pinBtn.onclick = function (e) {
    e.stopPropagation();
    if (window.__clewToggleFavorite) window.__clewToggleFavorite(id);
  };
  el.appendChild(head);

  if (st !== "mini") {
    const body = document.createElement("div");
    body.className = "body";
    if (n.kind === "class") {
      renderAccordion(id, body);
      el.appendChild(body);
      refreshSelectionUI(selectedId);
      return;
    }
    const shown = n.lines.length;
    const limit = st === "summary" ? SUMMARY : shown;
    n.lines.slice(0, limit).forEach(function (ln) { renderLine(ln, body, id); });
    if (st === "summary" && hasMore(id)) {
      const more = document.createElement("div");
      more.className = "ln more";
      more.textContent = "还有 " + (shown - SUMMARY) + " 行…";
      more.onclick = function (e) { e.stopPropagation(); state[id] = "full"; renderCard(id); relayout(); };
      body.appendChild(more);
    }

    el.appendChild(body);
  }
  refreshSelectionUI(selectedId);
}

// ─── 类卡片：手风琴。行按需生成（被调用到才出现），可回收 ────────
function methodSetOf(classId) {
  if (!methodSet[classId]) methodSet[classId] = new Set();
  return methodSet[classId];
}

function renderAccordion(classId, host) {
  const card = NODES[classId];
  // 构造行：只有这个类被实例化过才画；__init__ 沿 MRO 找，可能继承自父类
  const instantiated = (FANIN[classId] || 0) > 0;
  const ctorId = instantiated && card.ctor && NODES[card.ctor] ? card.ctor : null;
  if (ctorId) {
    const made = !!cardEls[ctorId];
    const inherited = card.ctor_owner && card.ctor_owner !== card.qualname;
    const box = document.createElement("div");
    box.className = "acc";
    const head = document.createElement("div");
    head.className = "acc-head";
    head.dataset.method = ctorId;
    head.innerHTML = '<span class="caret">' + (made ? "▣" : "▸") + '</span>' +
      '<span class="nm">' + esc(NODES[ctorId].title) + '</span>' +
      '<span class="cnt">' +
      (inherited ? "继承自 " + esc(card.ctor_owner) : "构造") + '</span>';
    head.title = made ? "定位到它的卡片" : "展开为卡片";
    head.onclick = function (e) {
      e.stopPropagation();
      if (cardEls[ctorId]) { select(ctorId); relayout(); }
      else expand(ctorId, classId);
    };
    box.appendChild(head);
    host.appendChild(box);
  } else if (instantiated && card.external_base) {
    // 构造来自项目外的基类：显示父类的 __init__ 名字，但不可展开
    const box = document.createElement("div");
    box.className = "acc";
    const head = document.createElement("div");
    head.className = "acc-head";
    head.innerHTML = '<span class="caret">·</span>' +
      '<span class="nm">' + esc(card.external_base) + '.__init__(…)</span>' +
      '<span class="cnt">构造 · 项目外</span>';
    head.title = card.external_base +
      " 是内置或第三方类型，Clew 索引不到它的实现";
    head.style.opacity = ".72";
    box.appendChild(head);
    host.appendChild(box);
  }
  // 方法目录：只列用户实际进入过的（点 ◆ 或点目录行产生）
  const set = methodSetOf(classId);
  if (set.size) {
    Array.from(set).sort().forEach(function (mid) {
      if (isCtor(mid)) return;           // 构造行是固定的，不重复列
      const m = NODES[mid];
      if (!m) return;
      const made = !!cardEls[mid];
      const box = document.createElement("div");
      box.className = "acc";
      const head = document.createElement("div");
      head.className = "acc-head";
      head.dataset.method = mid;
      head.innerHTML = '<span class="caret">' + (made ? "▣" : "▸") + '</span>' +
        '<span class="nm">' + esc(m.qualname.split(".").pop()) +
        '(' + m.params.join(", ") + ')</span>' +
        '<span class="cnt">被调 ' + (FANIN[mid] || 0) + '</span>';
      head.title = made ? "定位到它的卡片" : "展开为卡片";
      head.onclick = function (e) {
        e.stopPropagation();
        expand(mid, classId);
      };
      box.appendChild(head);
      host.appendChild(box);
    });
  } else {
    const note = document.createElement("div");
    note.className = "hidden-note";
    note.textContent = "还没有走过本类的方法（目录只收录走过的，点 ◆ 指进来才补）";
    host.appendChild(note);
  }
}
function ctorArgs(card) {
  return card.params && card.params.length ? "(" + card.params.join(", ") + ")" : "()";
}

// 方法不是卡片：展开 = 让所属类出现 + 该行出现 + 展开
function isMethod(id) {
  return NODES[id] && NODES[id].qualname.indexOf(".") !== -1;
}
function isCtor(id) {
  const n = NODES[id];
  return !!n && n.qualname.split(".").pop() === "__init__";
}

function classOf(id) {
  const n = NODES[id];
  return n.module + ":" + n.qualname.split(".")[0];
}
// ─── 放置 / 关闭 / 引用计数 ─────────────────────────────────────
function placeCard(id, parentHint) {
  if (!NODES[id] || cardEls[id]) return;
  const el = document.createElement("div");
  el.style.transition = "none";
  world.appendChild(el);
  cardEls[id] = el;
  state[id] = state[id] || "summary";
  const parent = parentHint || firstPlacedCaller(id);
  if (parent) parentOf[id] = parent;
  renderCard(id);
  requestAnimationFrame(function () { el.style.transition = ""; });
}

async function expand(id, parentHint, viaCs) {
  if (!id) return;
  if (!NODES[id]) {
    await ensure([id]);
    if (!NODES[id]) return;
  }
  if (viaCs) activated.add(viaCs);

  // 方法落地时，它的类卡要同时在图上：类目录和函数卡片一起出现，
  // 选中才能做到方法卡、类卡、目录行三处联动高亮
  if (isMethod(id)) {
    const cls = classOf(id);
    if (!isCtor(id)) methodSetOf(cls).add(id);
    if (!cardEls[cls] && NODES[cls]) placeCard(cls, parentHint);
    if (cardEls[cls]) renderCard(cls);   // 目录里补上这一行
  }

  placeCard(id, isMethod(id) && cardEls[classOf(id)] ? classOf(id) : parentHint);

  // 类被实例化就意味着构造会跑：类目录和 __init__ 卡片一起出来
  const self = NODES[id];
  if (self && self.kind === "class" && (FANIN[id] || 0) > 0 && self.ctor && NODES[self.ctor]) {
    placeCard(self.ctor, id);
  }

  select(id);
  relayout();
}

function firstPlacedCaller(id) {
  const k = Object.keys(CALLS).find(function (k) {
    return CALLS[k].to === id && cardEls[CALLS[k].from];
  });
  return k ? CALLS[k].from : null;
}

// 引用计数：谁还在引用这张卡
function incomingPlaced(id) {
  return Object.keys(CALLS).filter(function (k) {
    return CALLS[k].to === id && cardEls[CALLS[k].from] && CALLS[k].from !== id;
  }).map(function (k) { return CALLS[k].from; });
}

// 关闭：先移掉被点的卡片，再按引用计数剪掉失去所有引用的后代
function closeCard(id) {
  if (roots.has(id)) return;
  const children = Object.keys(CALLS)
    .filter(function (k) { return CALLS[k].from === id && CALLS[k].to; })
    .map(function (k) { return CALLS[k].to; });

  removeNode(id);

  children.forEach(function (child) {
    if (!cardEls[child] || roots.has(child)) return;
    if (incomingPlaced(child).length === 0) closeCard(child);
  });
  hideBubble();
  relayout();
}

function removeNode(id) {
  if (!cardEls[id]) return;
  if (NODES[id] && NODES[id].kind === "class") {
    Array.from(methodSet[id] || []).forEach(function (mid) { delete openMethods[mid]; });
    delete methodSet[id];
  }
  cardEls[id].remove();
  delete cardEls[id];
  delete openMethods[id];
  delete parentOf[id];
  delete state[id];
}

// ─── 布局：分层 + 同列按父位置排序（稳定顺序） ──────────────────
function layoutOrder() {
  const placed = Object.keys(cardEls);
  const placedSet = new Set(placed);
  const depth = {};
  const rootsPlaced = placed.filter(function (id) {
    const inc = incomingPlaced(id);
    return inc.length === 0;
  });
  rootsPlaced.forEach(function (r) { depth[r] = 0; });

  const queue = rootsPlaced.slice();
  const seen = new Set(queue);
  while (queue.length) {
    const cur = queue.shift();
    Object.keys(CALLS).forEach(function (k) {
      const c = CALLS[k];
      if (c.from !== cur || !c.to || !placedSet.has(c.to)) return;
      const d = depth[cur] + 1;
      if (depth[c.to] === undefined || depth[c.to] < d) depth[c.to] = d;
      if (!seen.has(c.to)) { seen.add(c.to); queue.push(c.to); }
    });
  }
  placed.forEach(function (id) { if (depth[id] === undefined) depth[id] = 0; });

  const cols = {};
  placed.forEach(function (id) {
    const d = depth[id];
    (cols[d] = cols[d] || []).push(id);
  });
  Object.keys(cols).forEach(function (d) {
    cols[d].sort(function (a, b) {
      const pa = avgParentY(a), pb = avgParentY(b);
      if (pa !== pb) return pa - pb;
      return a < b ? -1 : a > b ? 1 : 0;
    });
  });
  return { cols: cols, depth: depth };
}

function avgParentY(id) {
  const inc = incomingPlaced(id).filter(function (p) { return cardEls[p]; });
  if (!inc.length) return 0;
  let sum = 0;
  inc.forEach(function (p) { sum += Number(cardEls[p].style.top.replace("px", "")) || 0; });
  return sum / inc.length;
}

function relayout() {
  const { cols } = layoutOrder();
  const depths = Object.keys(cols).map(Number).sort(function (a, b) { return a - b; });
  let x = 40;
  depths.forEach(function (d) {
    let y = 50;
    cols[d].forEach(function (id) {
      const el = cardEls[id];
      if (!el) return;
      el.style.left = x + "px";
      el.style.top = y + "px";
      y += el.offsetHeight + ROW_GAP;
    });
    const widest = Math.max.apply(null, cols[d].map(function (id) {
      return (cardEls[id] && cardEls[id].offsetWidth) || CARD_W;
    }));
    x += widest + COL_GAP;
  });
  animateDraw(280);
}

// 过渡动画期间持续重画，否则线会停在旧位置
function animateDraw(ms) {
  const t0 = performance.now();
  (function step() {
    draw();
    if (performance.now() - t0 < ms) requestAnimationFrame(step);
  })();
}

// ─── 连线 ───────────────────────────────────────────────────────

function anchorOfCs(csId) {
  const nodes = world.querySelectorAll(".anchor");
  for (let i = 0; i < nodes.length; i++) {
    if (nodes[i].dataset && nodes[i].dataset.cs === csId) {
      const r = nodes[i].getBoundingClientRect(), w = world.getBoundingClientRect();
      return { x: r.left - w.left + r.width, y: r.top - w.top + r.height / 2 };
    }
  }
  return null;
}
function targetAnchor(cid) {
  if (cardEls[cid]) return leftPort(cid);
  if (isMethod(cid)) {
    const cls = classOf(cid);
    if (cardEls[cls]) return leftPort(cls);
  }
  return null;
}

function leftPort(id) {
  const el = cardEls[id];
  const r = el.getBoundingClientRect(), w = world.getBoundingClientRect();
  return { x: r.left - w.left, y: r.top - w.top + 15 };
}
function rightPort(id) {
  const el = cardEls[id];
  const r = el.getBoundingClientRect(), w = world.getBoundingClientRect();
  return { x: r.right - w.left, y: r.top - w.top + 15 };
}
function path(d, color, dash) {
  const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
  p.setAttribute("d", d);
  p.setAttribute("stroke", color);
  p.setAttribute("stroke-width", "1.3");
  p.setAttribute("fill", "none");
  if (dash) p.setAttribute("stroke-dasharray", "4 3");
  svg.appendChild(p);
}
function draw() {
  svg.innerHTML = "";
  const bus = {};
  Object.keys(CALLS).forEach(function (k) {
    const c = CALLS[k];
    if (!activated.has(k)) return;
    if (!c.to || !cardEls[c.from]) return;
    const b0 = targetAnchor(c.to);
    if (!b0) return;
    const a = anchorOfCs(k);
    const color = c.conf === "heur" ? "#d2a24c"
      : c.conf === "poly" ? "#bc8cff"
      : (c.from === c.to ? "#f0883e" : "#3fb950");
    const b = b0;
    if (!a) {
      // 行不在屏幕上（卡片折叠/方法收起）→ 从卡片右边缘合并出一条
      bus[c.from] = bus[c.from] || [];
      bus[c.from].push({ b: b, color: color });
      return;
    }
    const mx = (a.x + b.x) / 2;
    path("M " + a.x + " " + a.y + " C " + mx + " " + a.y + ", " + mx + " " + b.y + ", "
      + b.x + " " + b.y, color, c.from === c.to);
  });
  Object.keys(bus).forEach(function (id) {
    const a = rightPort(id), busX = a.x + 24;
    path("M " + a.x + " " + a.y + " L " + busX + " " + a.y, "#3a4652");
    bus[id].forEach(function (e) {
      const mx = Math.max(busX + 16, (busX + e.b.x) / 2);
      path("M " + busX + " " + a.y + " C " + mx + " " + a.y + ", " + mx + " " + e.b.y + ", "
        + e.b.x + " " + e.b.y, e.color);
    });
  });
}

// ─── 气泡 ───────────────────────────────────────────────────────
let hoverCs = null;
function argRows(cs) {
  const c = CALLS[cs];
  const params = NODES[c.to] ? NODES[c.to].params : [];
  if (!params.length) return '<div class="row"><span class="k">(无参数)</span><span class="v"></span></div>';
  return params.map(function (p) {
    const v = (c.args || {})[p];
    return '<div class="row"><span class="k">' + esc(p) + '</span><span class="v">' +
      (v === undefined ? "<span style='color:#6b7280'>未知</span>" : esc(v)) + "</span></div>";
  }).join("");
}
function confNote(c) {
  if (c.conf === "heur") return "启发式匹配（按名字唯一命中，可能错）";
  if (c.conf === "poly") return "多态：静态只知道方法名，下面是全部同名实现";
  return "精确匹配（同模块 / import / 类型推断 / self. 方法）";
}
function showBubble(cs, ev) {
  hoverCs = cs;
  const c = CALLS[cs];
  bubble.innerHTML = "";
  const hdr = document.createElement("div");
  hdr.className = "hdr";
  hdr.textContent = c.from + " → " + (c.to || c.target || "?") + (c.from === c.to ? " · 递归" : "");
  bubble.appendChild(hdr);
  const args = document.createElement("div");
  args.innerHTML = argRows(cs);
  bubble.appendChild(args);
  if (c.candidates && c.candidates.length > 1) {
    const list = document.createElement("div");
    list.style.marginTop = "5px";
    c.candidates.forEach(function (cid, i) {
      const ref = NODES[cid] || REFS[cid];
      if (!ref) return;
      const row = document.createElement("div");
      row.style.cssText = "padding:2px 4px;border-radius:3px;cursor:pointer;color:var(--acc)";
      row.textContent = (i + 1) + ". " + ref.module + " · " + ref.qualname;
      row.onmouseenter = function () { row.style.background = "#2d333b"; };
      row.onmouseleave = function () { row.style.background = "transparent"; };
      row.onclick = function () { hideBubble(); expand(cid, c.from, cs); };
      list.appendChild(row);
    });
    bubble.appendChild(list);
  }
  const note = document.createElement("div");
  note.className = "note";
  note.textContent = (c.typed ? "类型推断：" + c.typed + " · " : "") +
    (c.imported ? "来源：" + c.imported + " · " : "") + confNote(c);
  bubble.appendChild(note);
  bubble.style.display = "block";
  bubble.style.left = Math.min(ev.clientX + 14, window.innerWidth - 480) + "px";
  bubble.style.top = Math.min(ev.clientY + 14, window.innerHeight - 40 - bubble.offsetHeight) + "px";
}
function hideBubble() { hoverCs = null; bubble.style.display = "none"; }

// ─── 聚焦 / 选中 / 路径条 ───────────────────────────────────────
function openFocus(id) {
  focusLayer.style.display = "block";
  focusLayer.innerHTML = "";
  const clone = cardEls[id].cloneNode(true);
  clone.style.position = "relative";
  clone.style.left = "auto";
  clone.style.top = "auto";
  clone.querySelectorAll(".anchor, .btn").forEach(function (a) { a.remove(); });
  focusLayer.appendChild(clone);
}
function closeFocus() { focusLayer.style.display = "none"; focusLayer.innerHTML = ""; }
document.addEventListener("keydown", function (e) { if (e.key === "Escape") closeFocus(); });
focusLayer.addEventListener("click", function (e) { if (e.target === focusLayer) closeFocus(); });

let trail = [];
function chainOf(id) {
  const out = [], guard = new Set();
  let cur = id;
  while (cur && !guard.has(cur)) { guard.add(cur); out.unshift(cur); cur = parentOf[cur]; }
  return out;
}
function select(id) {
  selectedId = id;
  selectedRow = isMethod(id) ? id : null;   // __init__ 也参与
  refreshSelectionUI(selectedId);
  trail = chainOf(id);
  renderTrail();
}

// 单一入口：按 selectedId / selectedRow 重算全部高亮，不重建 DOM
function refreshSelectionUI(currentId) {
  Object.keys(cardEls).forEach(function (k) {
    const el = cardEls[k];
    if (!el || !el.classList) return;
    el.classList.toggle("sel", k === currentId);
    const owner = selectedRow ? classOf(selectedRow) : null;
    el.classList.toggle("related", !!owner && k === owner && k !== currentId);
  });
  const heads = world.querySelectorAll ? world.querySelectorAll(".acc-head[data-method]") : [];
  Array.prototype.forEach.call(heads, function (head) {
    head.classList.toggle("sel-row", selectedRow !== null && head.dataset.method === selectedRow);
  });
}
function renderTrail() {
  const el = document.getElementById("trail");
  el.innerHTML = "";
  trail.forEach(function (id, i) {
    const s = document.createElement("span");
    s.textContent = NODES[id] ? NODES[id].title : id;
    if (i === trail.length - 1) s.className = "cur";
    s.onclick = function () { expand(id); };
    el.appendChild(s);
    if (i < trail.length - 1) {
      const sep = document.createElement("i");
      sep.textContent = "›";
      el.appendChild(sep);
    }
  });
}

// ─── 根：每个根一份独立的图 ─────────────────────────────────────
const tabs = new Map();   // 根 card id -> 这份图的状态
let activeRoot = null;

function blankTab(rootId) {
  return {
    entryId: rootId,
    label: rootId,
    cardEls: {},
    state: {},
    openMethods: {},
    parentOf: {},
    methodSet: {},
    roots: new Set(),
    activated: new Set(),
    selectedId: null,
    selectedRow: null,
    trail: [],
    nodes: [],
  };
}

// 根只登记不预建：真正切到某个根时才建它的卡片
function ensureTab(rootId) {
  if (!rootId) return null;
  if (!tabs.has(rootId)) tabs.set(rootId, blankTab(rootId));
  return tabs.get(rootId);
}

function saveTab() {
  if (!activeRoot) return;
  const t = tabs.get(activeRoot);
  if (!t) return;
  t.cardEls = cardEls;
  t.state = state;
  t.openMethods = openMethods;
  t.parentOf = parentOf;
  t.methodSet = methodSet;
  t.roots = roots;
  t.activated = activated;
  t.selectedId = selectedId;
  t.selectedRow = selectedRow;
  t.trail = trail;
  t.nodes = Array.prototype.slice
    .call(world.children)
    .filter(function (el) { return el !== svg; });
}

function loadTab(rootId) {
  const t = ensureTab(rootId);
  if (!t) return;
  activeRoot = rootId;
  cardEls = t.cardEls;
  state = t.state;
  openMethods = t.openMethods;
  parentOf = t.parentOf;
  methodSet = t.methodSet;
  roots = t.roots;
  activated = t.activated;
  selectedId = t.selectedId;
  selectedRow = t.selectedRow;
  trail = t.trail;

  hideBubble();
  closeFocus();

  // 换掉画布里的卡片，svg 留着当连线层
  Array.prototype.slice.call(world.children).forEach(function (el) {
    if (el !== svg) world.removeChild(el);
  });
  t.nodes.forEach(function (el) { world.appendChild(el); });

  refreshSelectionUI(selectedId);
  renderTrail();
  relayout();
  if (window.__clewRootChanged) window.__clewRootChanged(rootId);
}

// 从入口面板打开任意一个符号作根。打开过就复用它的 tab。
async function openRoot(rootId) {
  if (!rootId || rootId === activeRoot) {
    if (rootId && window.__clewRootOpened) window.__clewRootOpened(rootId);
    return;
  }
  const t = ensureTab(rootId);
  if (!t) return;
  saveTab();
  loadTab(rootId);
  if (!t.nodes.length) {
    // 只有第一次开这个根才去取它的卡片
    roots.add(rootId);
    await expand(rootId);
    saveTab();
  }
  if (window.__clewRootOpened) window.__clewRootOpened(rootId);
}

window.openRoot = openRoot;
window.getActiveRoot = function () { return activeRoot; };
window.__clewRefreshCard = function (id) {
  if (cardEls[id]) renderCard(id);
};

// 换项目时整份换掉：数据、画布、每个根的状态全部重建
async function bootProject(entriesOrRoot) {
  // 兼容旧调用：传入口数组就取第一个作根，传字符串就是根本身，传空数组表示清空
  if (Array.isArray(entriesOrRoot)) ENTRIES = entriesOrRoot || [];
  // 预览模式的数据是整份 JSON，按需模式从空图开始，点开谁取谁
  NODES = LAZY ? {} : DATA.cards || {};
  CALLS = LAZY ? {} : DATA.calls || {};
  if (!LAZY) countFanin();

  hideBubble();
  closeFocus();
  Array.prototype.slice.call(world.children).forEach(function (el) {
    if (el !== svg) world.removeChild(el);
  });

  cardEls = {};
  state = {};
  openMethods = {};
  parentOf = {};
  methodSet = {};
  roots = new Set();
  activated = new Set();
  selectedId = null;
  selectedRow = null;
  trail = [];
  tabs.clear();
  activeRoot = null;

  const rootId = Array.isArray(entriesOrRoot)
    ? (ENTRIES.find(function (e) { return e && e.id; }) || {}).id
    : entriesOrRoot;

  if (rootId) {
    loadTab(rootId);
    roots.add(rootId);
    await expand(rootId);
    saveTab();
    if (window.__clewRootOpened) window.__clewRootOpened(rootId);
  } else if (window.__clewRootChanged) {
    window.__clewRootChanged(null);
  }

  if (window.__CLEW_MARK) {
    window.__CLEW_MARK("first-card");
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        window.__CLEW_MARK("paint");
        if (window.__CLEW_REPORT) window.__CLEW_REPORT();
      });
    });
  }
}

bootProject(ENTRIES).catch(function (err) {
  console.error("启动失败", err);
});

window.addEventListener("resize", function () { relayout(); });
graph.addEventListener("scroll", draw);


// ─── 画布拖拽 ───────────────────────────────────────────────────
(function panCanvas() {
  let panning = false;
  let startX = 0;
  let startY = 0;
  let startLeft = 0;
  let startTop = 0;

  // 卡片和控件上的按下要留给点击，别被平移吃掉
  function interactive(target) {
    return !!(
      target &&
      target.closest &&
      target.closest(".card, .acc-head, button, select, input, textarea")
    );
  }

  graph.addEventListener("mousedown", function (e) {
    if (e.button !== 0 || interactive(e.target)) return;
    panning = true;
    startX = e.clientX;
    startY = e.clientY;
    startLeft = graph.scrollLeft;
    startTop = graph.scrollTop;
    document.body.classList.add("panning");
    e.preventDefault();
  });

  window.addEventListener("mousemove", function (e) {
    if (!panning) return;
    graph.scrollLeft = startLeft - (e.clientX - startX);
    graph.scrollTop = startTop - (e.clientY - startY);
  });

  function stopPan() {
    if (!panning) return;
    panning = false;
    document.body.classList.remove("panning");
  }

  window.addEventListener("mouseup", stopPan);
  window.addEventListener("blur", stopPan);
})();


// ─── minimap ────────────────────────────────────────────────────
(function minimap() {
  const W = 200;
  const H = 150;
  const box = document.createElement("div");
  box.id = "minimap";
  box.title = "点或拖这里移动画布";
  const canvas = document.createElement("canvas");
  box.appendChild(canvas);
  document.body.appendChild(box);

  const ctx = canvas.getContext("2d");
  let frame = 0;
  let view = null;

  function fit() {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    canvas.style.width = W + "px";
    canvas.style.height = H + "px";
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }
  fit();

  // 卡片包围盒，留点边
  function bounds() {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    Object.keys(cardEls).forEach(function (id) {
      const el = cardEls[id];
      if (!el) return;
      const x = parseFloat(el.style.left) || 0;
      const y = parseFloat(el.style.top) || 0;
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x + (el.offsetWidth || 0));
      maxY = Math.max(maxY, y + (el.offsetHeight || 0));
    });
    if (!isFinite(minX)) return null;
    const pad = 90;
    return {
      x: minX - pad,
      y: minY - pad,
      w: maxX - minX + pad * 2,
      h: maxY - minY + pad * 2,
    };
  }

  function render() {
    ctx.clearRect(0, 0, W, H);
    const b = bounds();
    if (!b) {
      view = null;
      return;
    }
    const scale = Math.min(W / b.w, H / b.h);
    const ox = (W - b.w * scale) / 2;
    const oy = (H - b.h * scale) / 2;
    view = { scale: scale, ox: ox, oy: oy, b: b };

    const X = function (x) { return ox + (x - b.x) * scale; };
    const Y = function (y) { return oy + (y - b.y) * scale; };

    ctx.fillStyle = "#2b3440";
    ctx.strokeStyle = "#414d5d";
    ctx.lineWidth = 1;
    Object.keys(cardEls).forEach(function (id) {
      const el = cardEls[id];
      if (!el) return;
      const x = parseFloat(el.style.left) || 0;
      const y = parseFloat(el.style.top) || 0;
      const w = Math.max(2, (el.offsetWidth || 0) * scale);
      const h = Math.max(2, (el.offsetHeight || 0) * scale);
      ctx.fillRect(X(x), Y(y), w, h);
      ctx.strokeRect(X(x), Y(y), w, h);
    });

    // 视口可能比内容还大，裁到 minimap 范围内，否则会画出框外
    const vx0 = Math.max(0, Math.min(W, X(graph.scrollLeft)));
    const vy0 = Math.max(0, Math.min(H, Y(graph.scrollTop)));
    const vx1 = Math.max(0, Math.min(W, X(graph.scrollLeft + graph.clientWidth)));
    const vy1 = Math.max(0, Math.min(H, Y(graph.scrollTop + graph.clientHeight)));
    const vw = Math.max(5, vx1 - vx0);
    const vh = Math.max(5, vy1 - vy0);
    ctx.fillStyle = "rgba(88,166,255,0.14)";
    ctx.strokeStyle = "#58a6ff";
    ctx.lineWidth = 1.5;
    ctx.fillRect(vx0, vy0, vw, vh);
    ctx.strokeRect(vx0, vy0, vw, vh);
  }

  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(function () {
      frame = 0;
      render();
    });
  }

  let dragging = false;
  function jumpTo(e) {
    if (!view) return;
    const rect = canvas.getBoundingClientRect();
    const worldX = (e.clientX - rect.left - view.ox) / view.scale + view.b.x;
    const worldY = (e.clientY - rect.top - view.oy) / view.scale + view.b.y;
    graph.scrollLeft = worldX - graph.clientWidth / 2;
    graph.scrollTop = worldY - graph.clientHeight / 2;
    schedule();
  }

  canvas.addEventListener("mousedown", function (e) {
    if (e.button !== 0) return;
    dragging = true;
    jumpTo(e);
    e.preventDefault();
    e.stopPropagation();
  });
  window.addEventListener("mousemove", function (e) {
    if (dragging) jumpTo(e);
  });
  window.addEventListener("mouseup", function () {
    dragging = false;
  });

  graph.addEventListener("scroll", schedule);
  window.addEventListener("resize", function () {
    fit();
    schedule();
  });

  const originalRelayout = relayout;
  relayout = function () {
    originalRelayout.apply(null, arguments);
    schedule();
  };

  schedule();
})();
