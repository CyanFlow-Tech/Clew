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

# ── 3. 展开方法时同时落地它所属的类卡 ────────────────────────────
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

# ── 3. 构造行只在这个类真的被实例化过时才画 ──────────────────────
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

# ── 5. 启动就展开第一个入口，省一次点击 ──────────────────────────
js = (
    js.rstrip()
    + """

// ─── 启动：直接展开第一个入口 ─────────────────────────────
(function () {
  for (var i = 0; i < ENTRIES.length; i++) {
    var e = ENTRIES[i];
    if (e && e.id && NODES[e.id]) {
      roots.add(e.id);
      expand(e.id);
      return;
    }
  }
})();
"""
)

js = expect(js, "window.__CLEW__", "DATA swapped")
js = expect(js, "card.external_base", "ctor fallback")
js = expect(js, "还没有走过本类的方法", "directory wording")
if "declared" in js:
    raise SystemExit("method directory must not list unwalked definitions")
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
(out_dir / "index.html").write_text(html, encoding="utf-8")

print("wrote", out_dir / "app.js", len(js), "chars")
print("wrote", out_dir / "index.html", len(html), "chars")
