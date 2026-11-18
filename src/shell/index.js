// shell/index.js
// 外壳的入口。顺序就是原来闭包里自上而下的执行顺序：
// 每个模块在装载时建自己的 DOM、挂自己的事件，后面的模块要能拿到前面建好的节点。

import "./boot.js";
import "./util.js";
import "./welcome.js";
import "./confirm.js";
import "./titlebar.js";
import "./projectpanel.js";
import "./filemenu.js";
import "./help.js";
import "./project.js";
import "./startup.js";
import "./favorites.js";
import "./sidebar.js";
import "./settings.js";
import "./sidebase.js";
import "./sidesearch.js";
import "./resizer.js";
import "./viewmenu.js";
import "./dirtree.js";
import "./siderows.js";
import "./sidetoggle.js";
import "./sidetab.js";
