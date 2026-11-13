# Clew

> Every codebase is a maze. **Clew** is the thread.

A GUI code map for Python projects: start at an entry point, walk the call graph one step
at a time, and keep the trail of what you have read.

---

## Using Clew

### Open a project

The desktop window reads a SQLite index at `<project>/.clew/index.db`. The first open
builds it in the background and shows progress in the status bar; later opens read it
directly.

```bash
# release build (see "Building a release" for how to make one)
target\release\clew-app.exe E:\path\to\project

# development build, which also hot-reloads the frontend (see "Day to day")
.\tools\dev.ps1 -Project E:\path\to\project
```

Opening the window with `cargo run -p clew-app` alone is not enough in development: that
build loads the frontend from `http://127.0.0.1:1755`, so `tools/dev.ps1` has to be
running. A blank window means that server is not up.

### The window

- **Activity rail, far left.** Two tabs: the symbol tree (folder icon) and search
  (magnifier). Clicking the current tab again collapses the panel.
- **Sidebar.** A symbol tree of the whole project: folder, then file, then the
  definitions inside it. The dropdown at the top of the panel switches what it lists:

  | View | What it shows | Default |
  |---|---|---|
  | 全部函数 | every definition in the project | collapsed |
  | 入口函数 | entries the scanner found, one per line as `function` + file name | collapsed |
  | 收藏夹 | what you marked with ★ | flat list |
  | 最近使用 | cards you actually clicked in, newest first | flat list |

  Folders and files expand on click; the panel's right edge can be dragged to resize it,
  and the width is remembered.

- **Canvas.** Every card is one function, method or class: its code, line numbers, and one
  anchor per call site. Clicking an anchor opens the callee as a new card next to it. The
  breadcrumb in the title bar is the path you descended; the minimap in the corner can be
  clicked or dragged to move around.

- **★ marks an entry.** In the sidebar rows and on card headers. Marked cards show up
  under 收藏夹.

- **Search tab.** Two filters side by side: path and function name. Both are plain
  substring matches, one page of 50 at a time, "load more" at the bottom, and only the
  first hit highlights the matched fragment. Typing in the path box:
  - a bare string (`agent`) matches any single level — a directory or a file name;
  - a dotted or slashed path (`pkg.mod`, `/tests/agent`) walks the levels from the root,
    each level matched as a substring.

  **Ctrl+R** reloads the window.

---

## Developing Clew

### Repository layout

| Path | What lives here |
|---|---|
| `crates/clew-core` | parsing (tree-sitter), call resolution, index storage, search |
| `crates/clew-cli` | the `clew` command line; every capability is verifiable here first |
| `src-tauri` | window-side commands (`src-tauri/src/main.rs`) and the Tauri config |
| `src/` | frontend: `shell.js` is the shell and panels, `app.js` is the canvas, plain HTML + JS, no bundler, no npm |
| `tools/` | dev server, CDP harnesses, preview scripts |
| `prototype/` | standalone HTML mockups for UI experiments |
| `bench/` | the performance baseline script |

### Day to day

```powershell
.\tools\dev.ps1 -Project E:\path\to\project
```

That starts a static server on `http://127.0.0.1:1755` (with `Cache-Control: no-store`)
and runs the debug window against it, so `src/` is loaded at runtime instead of being
baked into the binary.

- **Editing html/js**: save, the window reloads itself within a second (it watches
  `index.html`, `app.js`, `shell.js`, `themes.js`, and appends a version query so the
  WebView cannot serve a cached script). `Ctrl+R` forces it.
- **Editing Rust**: stop the script and run it again. Debug builds are incremental and
  much faster than release.
- `dev.ps1` also opens the WebView2 debugging port (9222 by default, `-Cdp 0` turns it
  off), so `node tools/cdp.js "<js expression>"` can inspect the live window: DOM,
  computed styles, the current state of the panes.

### Building a release

```powershell
cargo app
```

That is an alias for `cargo build -p clew-app --release --features custom-protocol`,
defined in `.cargo/config.toml`. The `custom-protocol` feature is what makes Tauri embed
`src/` into the binary; without it the release build would try to load the dev server and
show a blank window, so `src-tauri/src/main.rs` refuses to compile in that state.

### Verifying without clicking around

```bash
# the index itself
cargo run -p clew-cli -- scan E:\path\to\project
cargo run -p clew-cli -- entries E:\path\to\project
cargo run -p clew-cli -- stats E:\path\to\project
cargo run -p clew-cli -- files E:\path\to\project
cargo run -p clew-cli -- tree E:\path\to\project --rows 40
cargo run -p clew-cli -- search E:\path\to\project 'curator:main'

# sidebar in a headless Edge over CDP
.\tools\side-check.ps1
```

It serves the real index data through a Tauri stub and asserts row counts, expansion
state and the panel geometry. The output is one JSON blob, so a change is a number, not
an impression.

### Working agreements

Read [AGENTS.md](AGENTS.md) before touching code: it covers asking before writing, real-data
acceptance, the rendering-before-scale rule, commit message format, and the rule that agents
verify the UI through the browser and CDP instead of launching the window themselves.

---

## The problem

Reading an unfamiliar Python codebase, the hard part is not "find the definition". It is:

- **Not knowing where to start.** Tens of thousands of symbols, and you do not know what
  to search for.
- **Navigation is linear.** An editor's back stack only rewinds; it cannot hold three open
  threads or tell you where you left off.
- **The whole graph is unreadable.** A complete call graph is too large, so editors show
  you one file and nothing else.

Clew makes the program's entry points the root of the map, lets you descend one call at a
time, and remembers the path you took.

---

## How it works

- A **card** is one function, method or class. It shows the code, line numbers, and one
  anchor per call site.
- An **anchor** carries the argument binding of that call (`param ← expression`), so you
  never have to match parameters to arguments by hand.
- **Classes are nodes too**, with their constructor and the methods you have actually
  entered.
- Every call is resolved as precisely as the syntax allows: same module, imports (including
  aliases and relative imports), `self.`/`cls.` methods, local variable types, and every
  candidate implementation when dispatch is dynamic.
- Imports are not drawn as edges. They show up as a styled name with its import path, so
  the graph stays a call graph instead of a dependency hairball.

---

## How it differs

|  | Graph viewers / LLM knowledge graphs | Compiler-based indexes | **Clew** |
|---|---|---|---|
| Where the graph comes from | LLM extraction or mixed AST | Compiler front end | **tree-sitter + ty, deterministic** |
| Built for | AI assistants | Machines | **Humans** |
| Starting point | Search box | None | **Program entry points** |
| Remembers where you were | No | No | **Yes, persisted across sessions** |
| Handling of scale | Spatial clustering | — | **Temporal: expand only what you walk into** |
| Notes | No | No | **Yes, and they survive renames** |

---

## Status

Early development. The indexing pipeline, call resolution, persistence, the interactive
walk, the CLI and the desktop GUI all work.

The GUI is the interaction model this project is built around: cards you open in place,
a `◆` on every call that shows what it bound, class cards that grow only the methods you
have actually entered, and the whole graph re-laid out as you go.

Not yet implemented: notes attached to cards (the table and the command exist, the UI does
not), visited/unvisited shading, type-aware resolution for `obj.method()` chains (planned
via `ty`), path export, and collapsing of single in/out chains in `walk`.

---

## Documentation

The design documents (architecture, engineering spec, onboarding notes, release
strategy) are kept outside the repository and written in Chinese for the
maintainer's own use. [bench/measure.py](bench/measure.py) reproduces the
performance baseline those documents quote.

## License

[MIT](LICENSE).
