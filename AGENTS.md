# 协作规则

这些规则来自 2026 年 11 月 12 日到 13 日那次搜索返工。夜里改了六轮，全部作废，原因是没问清语义就动手、拿假数据当验收、先放开数据量再做渲染。写在仓库根目录，任何 agent 进来先读这一篇。

## 一、先问后写

动任何代码之前，用一句话复述要做什么、影响哪里，等一句确认。没确认就停在那里，不写代码，不写半个函数当试探。

需求里出现两种解释的时候，问，不猜。猜错的成本由用户付。

## 二、真实数据验收

搜索、解析、索引这类改动，先在真实项目上跑出数字再报。

- 每个验收点写成可以在终端里复现的一行命令
- 报数字，命中数、耗时、内存
- 数字不对就不说「做好了」

单元测试只证明函数能跑。它证明不了真实项目里搜 `cli` 能不能落到 `hermes_cli`。造四个模块的样本说「测试全绿」，是自我安慰。

## 三、顺序不能反

先解决渲染，再放开数据量。先做虚拟列表与分页，再取消截断。反过来的结果就是一次返回五万八千条，界面直接卡死。

## 四、不自己发明

界面上的文案、语法、开关、默认行为、占位提示，一条都不加。要加先问。用户没要求的提示文字，就是要求之外的成本。

## 五、编译听指令

用户没说编译就不编译。说了也要先自己跑一遍真实查询，跑不通不交付。编译成功不等于功能可用。

## 六、被指出错误时先接住人

先回应对方此刻的处境，再谈技术。不抢着给方案，不解释理由，不拿「性能取舍」这类术语遮住失误。人被晾着比被争论更难受。

## 七、没有明确允许，不改代码，不提交

明确允许指的是用户说了可以改。方向讨论、方案确认、抱怨、疑问，都不构成允许。

提交同理：**用户说「提交」之前不动 `git commit`**。改完、测完、汇报完，停在那里等一句话。`git add` 也算动仓库，一起等。

## 八、项目背景

Clew 是 Python 项目的入口地图。Rust 后端用 tree-sitter 解析，数据落 SQLite，界面是 Tauri 2 加原生 JS。

- `crates/clew-core` 解析、索引、搜索
- `crates/clew-cli` 命令行，所有能力先在这里可验证
- `src-tauri` 窗口侧命令
- `src/` 前端，`shell.js` 是外壳与面板，`app.js` 是画布

日常开发用 `.\tools\dev.ps1 -Project <项目>`：它起 1755 上的静态服务，debug 版窗口直接连这个地址加载 `src/`，改 html/js 存盘页面自己刷新，不用重编（`shell.js` 盯着 `index.html`、`app.js`、`shell.js`、`themes.js` 四个文件的 `Last-Modified`，改完自动重载，也可 `Ctrl+R` 手动刷；脚本 URL 带版本串，绕开 WebView 缓存）。只有改 Rust 才重跑。

发版才用 `cargo app`（等价 `cargo build -p clew-app --release --features custom-protocol`）。release 必须带 `custom-protocol`，否则二进制里没有前端资源；`src-tauri/src/main.rs` 有编译期断言挡着，忘了带会直接报错而不是打出白窗。

不要自己启动窗口程序，验证界面用浏览器预览加 CDP 查 DOM。

## 九、提交信息

仓库历史就是这个格式，新提交照着写。写之前 `git log` 看两条，别按自己的习惯来。

```
feat(ui): add settings for where Clew keeps its data

Files:
- src-tauri/src/main.rs: builds the window in setup, adds the settings
  commands.
- src/shell.js: settings dialog and the gear on the activity rail.

Effects:
- WebView2 stops writing under %LOCALAPPDATA%.

What changed:
1. The data directory and the interface cache are configurable.
2. One page with two blocks; "default" only fills in the path.

Tests:
- cargo test -p clew-core.
- tools/settings-check.ps1.
```

- 全英文。单行 subject，空一行，然后 body。
- subject：`type(scope): ` 加一句祈使句，小写开头，不加句号，60 字符以内。type 用 `feat` / `fix` / `perf` / `refactor` / `chore` / `docs`；scope 用模块名，比如 `ui`、`scan`、`search`、`core`、`cli`。
- body 四段，顺序固定：
  1. `Files:` 逐条列改动的文件，每条一句说这个文件改了什么。
  2. `Effects:` 对用户可见的影响。没有就不写这一段。
  3. `What changed:` 分点写这次改了什么，一点一句，别写成散文。
  4. `Tests:` 跑了什么，有就写，没有就不写这一段。
- 短句优先。不写「我」，不写流水账，不复述 diff。
- 手工折行，每行 75 列左右。
- 一个提交只讲一件事。混进了别的工作，在 `What changed` 里单独一点交代它是什么、为什么一起进来。
- 不写 emoji，不写 `Co-authored-by`。

提交前把改动看一遍（`git status`、`git diff --stat`），确认 `prototype/tree-data.js` 这类导出数据没被带进去。

**提交和推送都要单独允许**：用户没说「提交」就不 commit，没说「推」就不 push。改完、测完、汇报完就停在那里，不要自己判断「顺手提交一下比较好」。

