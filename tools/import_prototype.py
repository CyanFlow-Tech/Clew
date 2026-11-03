"""Move the approved prototype into src/: static HTML + app.js, data via Tauri.

The prototype reads a single global `DATA` object. We keep the template verbatim
and only swap the injected literal for an async fetch of the same shape, plus a
few surgical patches listed below. Every patch fails loudly if the template
moves on, so a silent no-op can't ship again.
"""

import pathlib
import re

root = pathlib.Path(__file__).resolve().parent.parent
tmpl = root / "notes" / "prototypes" / "E-real.tmpl.html"
out_dir = root / "src"
out_dir.mkdir(exist_ok=True)

src = tmpl.read_text(encoding="utf-8")
match = re.search(r"<script>(.*?)</script>", src, re.S)
if not match:
    raise SystemExit("template has no <script> block")

js = match.group(1)


def patch(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"patch target not found: {label}")
    if new == old:
        raise SystemExit(f"patch is a no-op: {label}")
    return text.replace(old, new, 1)


def expect(text: str, needle: str, label: str) -> str:
    if needle not in text:
        raise SystemExit(f"expected output missing: {label}")
    return text


# ── 0. 数据来源：注入的字面量 → Tauri invoke ──────────────────────
js = patch(
    js,
    "const DATA = __DATA__;",
    "const DATA = window.__CLEW__;",
    "swap DATA literal",
)

# ── 1. 类卡片带上基类名：dotdict 一行就知道继承自 dict ──────────
js = patch(
    js,
    "    (n.kind === \"class\" ? '<span class=\"kindbadge\">类</span>' : '') +\n",
    "    (n.kind === \"class\" ? '<span class=\"kindbadge\">类</span>' : '') +\n"
    "    (n.kind === \"class\" && n.bases && n.bases.length ?\n"
    "      '<span class=\"kindbadge\">← ' + esc(n.bases.join(\", \")) + '</span>' : '') +\n",
    "class bases badge",
)

# ── 2. 空目录的提示改成陈述规则：目录只收录走过的，不列全部定义 ──
js = patch(
    js,
    """    note.textContent = "还没有进入任何方法（点别处的 ◆ 指到本类方法时会自动补上）";
""",
    """    note.textContent = "还没有走过本类的方法（目录只收录走过的，点 ◆ 指进来才补）";
""",
    "empty directory wording",
)

# ── 3. 构造行沿 MRO 找 __init__，不再只看本类 ────────────────────
# 原来只在本类方法里找 __init__，于是 dotdict 这种继承 dict 的类整行消失，
# 目录里只剩 __getattr__，看起来像构造就是它。
js = patch(
    js,
    """  const initRef = (card.methods || []).find(function (m) { return m.name === "__init__"; });

  // __init__ 和其他方法一样：一行，点开才是它自己的卡片
  if (initRef && NODES[initRef.target]) {
    const made = !!cardEls[initRef.target];
    const box = document.createElement("div");
    box.className = "acc";
    const head = document.createElement("div");
    head.className = "acc-head";
    head.dataset.method = initRef.target;
    head.innerHTML = '<span class="caret">' + (made ? "▣" : "▸") + '</span>' +
      '<span class="nm">' + esc(NODES[initRef.target].title) + '</span>' +
      '<span class="cnt">构造</span>';
    head.title = made ? "定位到它的卡片" : "展开为卡片";
    head.onclick = function (e) {
      e.stopPropagation();
      if (cardEls[initRef.target]) { select(initRef.target); relayout(); }
      else expand(initRef.target, classId);
    };
    box.appendChild(head);
    host.appendChild(box);
  }
""",
    """  // 构造行：沿 MRO 找到的 __init__，可能是从父类继承来的
  const ctorId = card.ctor && NODES[card.ctor] ? card.ctor : null;
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
  } else if (card.external_base) {
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
""",
    "ctor row via MRO",
)

# ── 3b. 展开方法时同时落地它所属的类卡 ───────────────────────────
# 原来只有类卡已经在图上时才刷新目录；从 ◆ 直接进一个方法，就只有函数卡片、
# 没有类目录，也就没有「方法卡 + 类卡 + 目录行」三处联动高亮。
js = patch(
    js,
    """function expand(id, parentHint, viaCs) {
  if (!NODES[id]) return;
  if (viaCs) activated.add(viaCs);
  if (isMethod(id) && !isCtor(id)) {
    const cls = classOf(id);
    methodSetOf(cls).add(id);
    if (cardEls[cls]) renderCard(cls);   // 目录里补上这一行
  }

  if (!cardEls[id]) {
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
  select(id);
  relayout();
}
""",
    """function placeCard(id, parentHint) {
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

function expand(id, parentHint, viaCs) {
  if (!NODES[id]) return;
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
  select(id);
  relayout();
}
""",
    "expand places the owning class card",
)

# ── 3c. 构造行只在这个类真的被实例化过时才画 ─────────────────────
# 原来只要有 __init__ 就画，于是从未被调用的类（Game、NeuralNet、各种
# RandomPlayer）也会挂着一条构造行，违反「目录只收录走过的」。
js = patch(
    js,
    """  // 构造行：沿 MRO 找到的 __init__，可能是从父类继承来的
  const ctorId = card.ctor && NODES[card.ctor] ? card.ctor : null;
  if (ctorId) {
""",
    """  // 构造行：只有这个类被实例化过才画；__init__ 沿 MRO 找，可能继承自父类
  const instantiated = (FANIN[classId] || 0) > 0;
  const ctorId = instantiated && card.ctor && NODES[card.ctor] ? card.ctor : null;
  if (ctorId) {
""",
    "ctor row only when instantiated",
)

# 3b. 外部基类那条分支同样只在实例化过时出现
js = patch(
    js,
    "  } else if (card.external_base) {",
    "  } else if (instantiated && card.external_base) {",
    "external ctor row only when instantiated",
)

# ── 4. 展开类卡时把它的构造一起放出来 ────────────────────────────
# `MCTS(...)` 点开只出类目录、不出 __init__ 卡片，读的时候还得再点一次。
js = patch(
    js,
    """  placeCard(id, isMethod(id) && cardEls[classOf(id)] ? classOf(id) : parentHint);
  select(id);
  relayout();
}
""",
    """  placeCard(id, isMethod(id) && cardEls[classOf(id)] ? classOf(id) : parentHint);

  // 类被实例化就意味着构造会跑：类目录和 __init__ 卡片一起出来
  const self = NODES[id];
  if (self && self.kind === "class" && (FANIN[id] || 0) > 0 && self.ctor && NODES[self.ctor]) {
    placeCard(self.ctor, id);
  }

  select(id);
  relayout();
}
""",
    "expand class brings its constructor",
)

# ── 5. 图状态改成可切换：每个入口 tab 一份 ────────────────────────
# 这些容器原来是 const，切 tab 时整体换进换出，所以得能重新赋值。
js = patch(
    js,
    """const cardEls = {};       // 已放置的卡片 DOM
const state = {};         // summary | full | mini
const openMethods = {};   // 类卡片里展开的方法
const parentOf = {};      // 放置时的父卡片
const methodSet = {};     // 类卡片 -> 已出现的成员方法（按需生成）
const activated = new Set(); // 被用户实际跟进过的调用点（cs id）
""",
    """let cardEls = {};         // 已放置的卡片 DOM
let state = {};           // summary | full | mini
let openMethods = {};     // 类卡片里展开的方法
let parentOf = {};        // 放置时的父卡片
let methodSet = {};       // 类卡片 -> 已出现的成员方法（按需生成）
let activated = new Set();   // 被用户实际跟进过的调用点（cs id）
""",
    "graph state is swappable",
)

js = patch(
    js,
    "const roots = new Set();  // 入口卡片\n",
    "let roots = new Set();    // 入口卡片\n",
    "roots is swappable",
)


# ── 6. 入口切换：每个入口一份独立的图，首次进入展开它的入口卡片 ────
js = patch(
    js,
    """// ─── 入口 ───────────────────────────────────────────────────────
ENTRIES.slice(0, 12).forEach(function (e) {
  if (!e.id || !NODES[e.id]) return;
  const b = document.createElement("button");
  b.className = "ent";
  b.textContent = e.label;
  b.onclick = function () {
    roots.add(e.id);
    expand(e.id);
  };
  document.getElementById("entries").appendChild(b);
});
""",
    """// ─── 入口 tab：每个入口一份独立的图 ─────────────────────────────
const tabs = [];
let activeTab = -1;

function blankTab(entry) {
  return {
    entryId: entry.id,
    label: entry.label,
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

function saveTab() {
  if (activeTab < 0) return;
  const t = tabs[activeTab];
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

function loadTab(i) {
  const t = tabs[i];
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

  activeTab = i;
  renderTabs();
  refreshSelectionUI(selectedId);
  renderTrail();
  relayout();
}

function switchTab(i) {
  if (i === activeTab || !tabs[i]) return;
  saveTab();
  loadTab(i);
  if (!tabs[i].nodes.length) {
    roots.add(tabs[i].entryId);
    expand(tabs[i].entryId);   // 首次进这个 tab：默认显示它的入口卡片
    saveTab();
  }
}

function renderTabs() {
  const box = document.getElementById("entries");
  box.innerHTML = "";
  tabs.forEach(function (t, i) {
    const b = document.createElement("button");
    b.className = "ent" + (i === activeTab ? " active" : "");
    b.textContent = t.label;
    b.title = t.entryId;
    b.onclick = function () { switchTab(i); };
    box.appendChild(b);
  });
}

ENTRIES.slice(0, 12).forEach(function (e) {
  if (!e || !e.id || !NODES[e.id]) return;
  tabs.push(blankTab(e));
});

if (tabs.length) {
  loadTab(0);
  roots.add(tabs[0].entryId);
  expand(tabs[0].entryId);
  saveTab();
}
""",
    "entry tabs",
)
# ── 7. 入口栏改成下拉选择：入口多的时候 tab 会溢出成滚动条 ────────
js = patch(
    js,
    """function renderTabs() {
  const box = document.getElementById("entries");
  box.innerHTML = "";
  tabs.forEach(function (t, i) {
    const b = document.createElement("button");
    b.className = "ent" + (i === activeTab ? " active" : "");
    b.textContent = t.label;
    b.title = t.entryId;
    b.onclick = function () { switchTab(i); };
    box.appendChild(b);
  });
}
""",
    """let entrySelect = null;

function renderTabs() {
  const box = document.getElementById("entries");
  if (!entrySelect) {
    entrySelect = document.createElement("select");
    entrySelect.className = "ent-select";
    entrySelect.title = "选择要看的入口";
    tabs.forEach(function (t, i) {
      const o = document.createElement("option");
      o.value = String(i);
      o.textContent = t.label.replace(":<module>", "");
      o.title = t.label;
      entrySelect.appendChild(o);
    });
    entrySelect.onchange = function () {
      switchTab(Number(entrySelect.value));
    };
    box.appendChild(entrySelect);
  }
  entrySelect.value = String(activeTab < 0 ? 0 : activeTab);
}
""",
    "entry selector",
)
js = expect(js, "window.__CLEW__", "DATA swapped")
js = expect(js, "card.external_base", "ctor fallback")
js = expect(js, "还没有走过本类的方法", "directory wording")
js = expect(js, "function switchTab", "entry tabs")
if "declared" in js:
    raise SystemExit("method directory must not list unwalked definitions")

# ── 8. 画布拖拽平移：抓手按住就移动，不用去够滚动条 ──────────────
# 只挪 scrollLeft/scrollTop，布局和 relayout 完全不受影响。
js += """

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
"""

js = expect(js, "panCanvas", "canvas pan")

# ── 9. minimap：滚动条藏起来之后，靠它判断自己在画布哪儿 ──────────
js += """

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
"""

js = expect(js, "function minimap", "minimap")
(out_dir / "app.js").write_text(js, encoding="utf-8")

loader = """<script>
  // 数据来自索引库：先打开项目，再一次性取出整张图，最后加载界面脚本
  const api = window.__TAURI__.core;

  function clewFail(msg) {
    document.body.innerHTML =
      '<pre style="color:#f88;padding:24px;white-space:pre-wrap">' + msg + "</pre>";
  }

  window.onerror = function (msg, src, line, col, err) {
    clewFail(msg + "\\n" + (src || "") + ":" + line + ":" + col +
             "\\n" + ((err && err.stack) || ""));
  };
  window.addEventListener("unhandledrejection", function (ev) {
    clewFail("promise: " + String((ev.reason && ev.reason.stack) || ev.reason));
  });

  function clewPrompt() {
    const overlay = document.createElement("div");
    overlay.style.cssText =
      "position:fixed;inset:0;background:#14161af2;display:grid;place-items:center;" +
      "z-index:99;font:14px ui-monospace,Consolas,monospace;color:#d6dae1";
    overlay.innerHTML =
      '<div><p>输入已索引的项目路径，回车打开（没索引过先跑 clew scan）：</p>' +
      '<input id="clew-path" spellcheck="false" style="width:60vw;padding:8px;' +
      'background:#0f1115;color:#d6dae1;border:1px solid #2a2f38;font:inherit">' +
      '<p id="clew-err" style="color:#f88;white-space:pre-wrap"></p></div>';
    document.body.appendChild(overlay);

    const input = overlay.querySelector("#clew-path");
    const err = overlay.querySelector("#clew-err");
    input.focus();
    input.onkeydown = function (ev) {
      if (ev.key !== "Enter") return;
      api
        .invoke("open_project", { path: input.value.trim() })
        .then(function () {
          overlay.remove();
          return clewMount();
        })
        .catch(function (e) {
          err.textContent = String(e);
        });
    };
  }

  function clewMount() {
    return api.invoke("graph").then(function (data) {
      window.__CLEW__ = data;
      const s = document.createElement("script");
      s.src = "app.js";
      s.onerror = function () {
        clewFail("app.js 加载失败");
      };
      document.body.appendChild(s);
    });
  }

  api
    .invoke("boot")
    .then(function (info) {
      if (info) return clewMount();
      clewPrompt();
    })
    .catch(clewFail);
</script>"""

html = src[: match.start()] + loader + src[match.end() :]
html = patch(
    html,
    "  .ent:hover{border-color:var(--acc);color:var(--acc)}",
    "  .ent:hover{border-color:var(--acc);color:var(--acc)}\n"
    "  .ent-select{background:#1f2630;border:1px solid var(--bd);border-radius:4px;\n"
    "    padding:2px 6px;color:var(--tx);font:inherit;font-size:11px;max-width:46vw}\n"
    "  .ent-select:hover{border-color:var(--acc)}\n"
    "  .ent-select:focus{outline:1px solid var(--acc)}",
    "entry selector style",
)
html = patch(
    html,
    "#graph{position:absolute;inset:42px 0 0 0;overflow:auto}",
    "#graph{position:absolute;inset:42px 0 0 0;overflow:auto;cursor:grab;"
    "scrollbar-width:none}\n"
    "  #graph::-webkit-scrollbar{display:none;width:0;height:0}\n"
    "  body.panning,body.panning *{cursor:grabbing!important;user-select:none}\n"
    "  #minimap{position:fixed;right:14px;bottom:16px;z-index:30;padding:6px;\n"
    "    background:#0f1216e6;border:1px solid var(--bd);border-radius:6px;\n"
    "    box-shadow:0 6px 18px #0007;cursor:pointer;line-height:0}\n"
    "  #minimap:hover{border-color:var(--acc)}",
    "canvas pan cursor",
)
(out_dir / "index.html").write_text(html, encoding="utf-8")

print("wrote", out_dir / "app.js", len(js), "chars")
print("wrote", out_dir / "index.html", len(html), "chars")
