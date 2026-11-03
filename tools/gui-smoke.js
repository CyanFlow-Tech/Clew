// Boot the app, assert the rendered graph over CDP, then quit.
//
//   node tools/gui-smoke.js <project-path> [exe-path]
//
// Checks that hold for any project:
//   - the graph has cards
//   - every class card either has a resolvable constructor, a recorded base
//     outside the project, or no declared methods at all
//   - every constructor / base card id actually exists
//   - class cards render a body without throwing
//
// Prints the offending classes instead of making you click through them.

const { spawn } = require("node:child_process");
const path = require("node:path");

const PORT = process.env.CLEW_CDP_PORT || 9222;
const project = process.argv[2];
const exe = process.argv[3] || path.join(__dirname, "..", "target", "debug", "clew-app.exe");

if (!project) {
  console.error("usage: node tools/gui-smoke.js <project-path> [exe-path]");
  process.exit(2);
}

const PROBE = `(() => {
  const problems = [];
  let cards = 0;
  let classes = 0;
  let methods = 0;

  // 0) 每个入口一个 tab，切进去就要有自己的入口卡片
  const tabCount = tabs.length;
  if (tabCount !== Math.min(ENTRIES.length, 12)) {
    problems.push(["tab 数与入口数不符", String(tabCount), String(ENTRIES.length)]);
  }
  for (let i = 0; i < tabs.length; i++) {
    switchTab(i);
    const t = tabs[i];
    if (!t.nodes.length) problems.push(["tab 没有默认卡片", t.label, ""]);
    if (!cardEls[t.entryId]) problems.push(["tab 缺入口卡片", t.label, ""]);
    const others = Object.keys(cardEls).filter((k) => k !== t.entryId);
    if (others.length) {
      problems.push(["tab 混入了别的入口的卡片", t.label, others.slice(0, 3).join(",")]);
    }
  }
  switchTab(0);

  // 画布要能抓手拖拽：光标对、按住移动会改变滚动位置
  const canvas = document.getElementById("graph");
  if (!canvas) {
    problems.push(["找不到画布", "", ""]);
  } else {
    if (getComputedStyle(canvas).cursor !== "grab") {
      problems.push(["画布没有抓手光标", getComputedStyle(canvas).cursor, ""]);
    }
    const beforePan = { left: canvas.scrollLeft, top: canvas.scrollTop };
    canvas.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, clientX: 600, clientY: 600, button: 0 }),
    );
    window.dispatchEvent(
      new MouseEvent("mousemove", { bubbles: true, clientX: 480, clientY: 520 }),
    );
    window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    const afterPan = { left: canvas.scrollLeft, top: canvas.scrollTop };
    if (afterPan.left === beforePan.left && afterPan.top === beforePan.top) {
      problems.push(["拖拽没有移动画布", JSON.stringify(beforePan), JSON.stringify(afterPan)]);
    }

    // 卡片上的按下要留给点击，不能被平移吃掉
    const card = canvas.querySelector(".card");
    if (card) {
      const left = canvas.scrollLeft;
      card.dispatchEvent(
        new MouseEvent("mousedown", { bubbles: true, clientX: 600, clientY: 600, button: 0 }),
      );
      window.dispatchEvent(
        new MouseEvent("mousemove", { bubbles: true, clientX: 400, clientY: 400 }),
      );
      window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
      if (canvas.scrollLeft !== left) {
        problems.push(["卡片上拖动误触了平移", "", ""]);
      }
    }

    // 滚动条藏起来了，minimap 必须存在并且能点着移动画布
    if (getComputedStyle(canvas).scrollbarWidth !== "none") {
      problems.push(["滚动条没隐藏", getComputedStyle(canvas).scrollbarWidth, ""]);
    }
    const mini = document.getElementById("minimap");
    if (!mini) {
      problems.push(["缺少 minimap", "", ""]);
    } else {
      const mc = mini.querySelector("canvas");
      if (!mc || !mc.width || !mc.height) {
        problems.push(["minimap 没画出来", "", ""]);
      } else {
        const before = canvas.scrollLeft;
        const rect = mc.getBoundingClientRect();
        mc.dispatchEvent(
          new MouseEvent("mousedown", {
            bubbles: true,
            clientX: rect.left + rect.width * 0.85,
            clientY: rect.top + rect.height * 0.85,
            button: 0,
          }),
        );
        window.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
        if (canvas.scrollLeft === before) {
          problems.push(["点 minimap 没移动画布", String(before), String(canvas.scrollLeft)]);
        }
      }
    }
  }

  const sel = document.querySelector(".ent-select");
  if (!sel) {
    problems.push(["缺少入口下拉", "", ""]);
  } else {
    if (sel.options.length !== tabs.length) {
      problems.push(["下拉项数与入口不符", String(sel.options.length), String(tabs.length)]);
    }
    if (Number(sel.value) !== activeTab) {
      problems.push(["下拉没跟上当前入口", sel.value, String(activeTab)]);
    }
  }

  // 1) 每个类卡：构造行只出现在被实例化过的类上；目录只收录走过的
  for (const [id, c] of Object.entries(NODES)) {
    cards++;
    if (c.kind !== "class") continue;
    classes++;

    let text = "";
    try {
      const host = document.createElement("div");
      renderAccordion(id, host);
      text = host.innerText;
    } catch (e) {
      problems.push(["渲染抛错", id, String(e && e.message)]);
      continue;
    }

    const fan = FANIN[id] || 0;
    const wantsCtorRow = fan > 0 && !!(c.ctor || c.external_base);
    const hasCtorRow = text.indexOf(".__init__") >= 0;
    if (wantsCtorRow && !hasCtorRow) {
      problems.push(["实例化过却没有构造行", id, "被调 " + fan]);
    }
    if (!wantsCtorRow && hasCtorRow) {
      problems.push(["没实例化却画了构造行", id, text.slice(0, 50)]);
    }
    if (c.external_base && fan > 0 && text.indexOf(c.external_base + ".__init__") < 0) {
      problems.push(["外部构造没写父类名", id, text.slice(0, 50)]);
    }
    if (text.indexOf("未被调用") >= 0 || text.indexOf("未进入") >= 0) {
      problems.push(["目录列了没走过的定义", id, text.slice(0, 50)]);
    }
    if (((methodSet[id] && methodSet[id].size) || 0) === 0 && text.indexOf("被调 ") >= 0) {
      problems.push(["空目录却渲染了方法行", id, text.slice(0, 50)]);
    }
    if (c.ctor && !NODES[c.ctor]) {
      problems.push(["ctor 指向不存在的卡", id, c.ctor]);
    }
  }

  // 布局在批量探测时没有意义，关掉以免 O(n^2)
  const realRelayout = window.relayout;
  window.relayout = function () {};

  // 2) 展开一个被实例化过的类，它的构造卡要跟着落地
  for (const [id, c] of Object.entries(NODES)) {
    if (c.kind !== "class" || !c.ctor || !NODES[c.ctor]) continue;
    if (!((FANIN[id] || 0) > 0)) continue;
    if (cardEls[c.ctor]) removeNode(c.ctor);
    expand(id);
    if (!cardEls[c.ctor]) problems.push(["展开类没带出构造卡", id, c.ctor]);
  }

  // 3) 每个普通方法：方法卡 + 类卡 + 目录行 + 两处高亮，一个都不能少
  for (const id of Object.keys(NODES)) {
    if (NODES[id].kind !== "method" || isCtor(id)) continue;
    const cls = classOf(id);
    if (!NODES[cls]) continue;
    methods++;

    expand(id);
    const classCard = cardEls[cls];
    const row = classCard
      ? classCard.querySelector('.acc-head[data-method="' + id + '"]')
      : null;

    if (!cardEls[id]) problems.push(["方法卡没出现", id, ""]);
    if (!classCard) problems.push(["类卡没出现", id, ""]);
    if (!row) problems.push(["目录缺这一行", id, ""]);
    if (row && !row.classList.contains("sel-row")) {
      problems.push(["目录行没高亮", id, ""]);
    }
    if (classCard && !classCard.classList.contains("related")) {
      problems.push(["类卡没联动高亮", id, ""]);
    }
  }

  window.relayout = realRelayout;

  return JSON.stringify({ cards, classes, methods, problems });
})()`;

async function main() {
  const child = spawn(exe, [project], {
    cwd: path.join(__dirname, ".."),
    stdio: "ignore",
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  });

  const stop = () => {
    try {
      child.kill();
    } catch {
      /* already gone */
    }
  };
  process.on("exit", stop);

  let target = null;
  for (let i = 0; i < 120 && !target; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      target = list.find((t) => t.type === "page") || null;
    } catch {
      /* not listening yet */
    }
    if (!target) await new Promise((r) => setTimeout(r, 250));
  }
  if (!target) {
    console.error("CDP never came up");
    stop();
    process.exit(1);
  }

  // the page loads its data asynchronously; wait for the graph to land
  let data = null;
  for (let i = 0; i < 60 && !data; i++) {
    const probe = await evaluate(target, "typeof NODES !== 'undefined' && Object.keys(NODES).length");
    if (probe > 0) data = probe;
    else await new Promise((r) => setTimeout(r, 250));
  }
  if (!data) {
    console.error("graph never loaded into the page");
    stop();
    process.exit(1);
  }

  const raw = await evaluate(target, PROBE);
  const report = JSON.parse(raw);

  console.log(`cards      ${report.cards}`);
  console.log(`classes    ${report.classes}`);
  console.log(`methods    ${report.methods}`);
  console.log(`problems   ${report.problems.length}`);

  const shown = report.problems.slice(0, 20);
  for (const [kind, id, detail] of shown) {
    console.log(`  ✗ ${kind.padEnd(18)} ${id}  ${detail}`);
  }
  if (report.problems.length > shown.length) {
    console.log(`  … 还有 ${report.problems.length - shown.length} 条`);
  }

  stop();
  process.exit(report.problems.length ? 1 : 0);
}

async function evaluate(target, expression) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  const id = 1;
  const done = new Promise((resolve) => {
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id === id) resolve(msg);
    });
  });

  ws.send(
    JSON.stringify({
      id,
      method: "Runtime.evaluate",
      params: { expression, returnByValue: true, awaitPromise: true },
    }),
  );

  const msg = await done;
  ws.close();

  const exc = msg.result && msg.result.exceptionDetails;
  if (exc) throw new Error(exc.exception?.description || exc.text);
  return msg.result?.result?.value;
}

main().catch((e) => {
  console.error(e.message || e);
  process.exit(1);
});
