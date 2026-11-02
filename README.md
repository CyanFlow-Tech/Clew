# Clew

> Every codebase is a maze. **Clew** is the thread.

A GUI code map for Python projects: start at an entry point, walk the call graph one step
at a time, and keep the trail of what you have read.

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

## What works today

The Rust core, the CLI, and a Tauri desktop shell with a first-pass GUI.

```bash
# Build the index for a project
cargo run -p clew-cli -- scan /path/to/project

# List discovered entry points
cargo run -p clew-cli -- entries /path/to/project

# Read the counters back from the index
cargo run -p clew-cli -- stats /path/to/project

# Show the argument bindings of the calls on one line of one function
cargo run -p clew-cli -- args /path/to/project 'Class.method' 42

# Walk the call graph interactively from an entry point
cargo run -p clew-cli -- walk /path/to/project

# Open the desktop app, which reads the same index
cargo run -p clew-app -- /path/to/project
```

`scan` writes a SQLite index at `<project>/.clew/index.db`; that database is the single
source of truth the GUI reads from. There is no intermediate JSON, and the frontend is
plain HTML + JS loaded straight out of `src/` — no bundler, no npm.

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

Not yet implemented: notes attached to cards (the table and the command exist, the UI
does not), visited/unvisited shading, type-aware resolution for `obj.method()` chains
(planned via `ty`), path export, and collapsing of single in/out chains in `walk`.

---

## Documentation

The design documents (architecture, engineering spec, onboarding notes, release
strategy) are kept outside the repository and written in Chinese for the
maintainer's own use. [bench/measure.py](bench/measure.py) reproduces the
performance baseline those documents quote.

## License

[MIT](LICENSE).
