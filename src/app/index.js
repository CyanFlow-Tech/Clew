// app/index.js
// 画布的入口。模块之间用 import 说明依赖，这里把没有人引用的几个按顺序拿进来：
// 装载顺序不对，画布会在别人的模块还没准备好时就去读它的值。

import "./env.js";
import "./dom.js";
import "./store.js";
import "./data.js";
import "./format.js";
import "./highlight.js";
import "./card.js";
import "./place.js";
import "./layout.js";
import "./edge.js";
import "./select.js";
import "./workspace.js";
import "./loadtab.js";
import "./canvas.js";

// 调试接口：只交出下面这几个名字，模块内部状态不往 window 上撒。
import { store } from "./store.js";
import { expand, removeNode } from "./place.js";
import { classOf, isCtor, renderAccordion } from "./card.js";

window.CLEW_DEBUG = { store, expand, removeNode, renderAccordion, isCtor, classOf };
