// shell/dirtree.js
import { store } from "./store.js";
import { basename } from "./util.js";
import { splitId } from "./id.js";
import { st } from "./sidebar.js";

export function nestSyms(syms) {
  const byQ = Object.create(null);
  syms.forEach(function (s) {
    byQ[s.qualname] = { sym: s, kids: [] };
  });
  const tops = [];
  syms.forEach(function (s) {
    const node = byQ[s.qualname];
    const dot = s.qualname.lastIndexOf(".");
    const parent = dot > 0 ? byQ[s.qualname.slice(0, dot)] : null;
    if (parent && parent !== node) parent.kids.push(node);
    else tops.push(node);
  });
  return { tops: tops, byQ: byQ };
}

export function emitNode(rows, node, d) {
  const s = node.sym;
  const open = st().syms.has(s.card_id);
  rows.push({
    kind: "sym",
    depth: d,
    card: s.card_id,
    module: s.module,
    qual: s.qualname,
    kindName: s.kind,
    kids: node.kids.length,
    open: open,
    isEntry: !!s.is_entry,
    line: s.def_line || 0,
  });
  if (open) node.kids.forEach((k) => emitNode(rows, k, d + 1));
}

export function pushNested(rows, module, syms, depth) {
  const nested = nestSyms(syms);
  nested.tops.forEach((n) => emitNode(rows, n, depth));
}

// ── 目录树 ──────────────────────────────────────────────────────
// 树上三层：文件夹 → 文件 → 定义。文件夹按相对路径切出来，文件挂在
// 它所在的目录下，函数和类仍然按点号嵌套在文件里。
export function buildDirTree(items) {
  const root = { name: "", dirs: new Map(), files: [] };
  items.forEach(function (it) {
    const path = it.path || it.module;
    const parts = path.split("/");
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const name = parts[i];
      if (!node.dirs.has(name)) {
        node.dirs.set(name, { name: name, dirs: new Map(), files: [] });
      }
      node = node.dirs.get(name);
    }
    node.files.push({
      module: it.module,
      path: path,
      third: !!it.third,
      symbols: it.symbols || [],
    });
  });
  // 目录一个都不展开的话，进来的第一眼只是一排文件夹，看不到函数
  const nodes = new Map();
  (function walk(node, prefix) {
    nodes.set(prefix, node);
    node.dirs.forEach(function (child, name) {
      walk(child, prefix ? prefix + "/" + name : name);
    });
  })(root, "");
  return { root: root, nodes: nodes };
}

export function emitDir(node, prefix, depth, rows) {
  const names = Array.from(node.dirs.keys()).sort(function (a, b) {
    return a.toLowerCase() < b.toLowerCase() ? -1 : 1;
  });
  names.forEach(function (name) {
    const child = node.dirs.get(name);
    const key = prefix ? prefix + "/" + name : name;
    const open = st().dirs.has(key);
    rows.push({ kind: "dir", depth: depth, dir: key, name: name, open: open });
    if (open) emitDir(child, key, depth + 1, rows);
  });
  node.files
    .slice()
    .sort(function (a, b) {
      const x = basename(a.path).toLowerCase();
      const y = basename(b.path).toLowerCase();
      return x < y ? -1 : x > y ? 1 : 0;
    })
    .forEach(function (f) {
      // 入口视图不再有文件这一层：一行函数名，下面一行小字是文件名（当注释看）
      if (store.spView === "entry") {
        f.symbols.forEach(function (s) {
          rows.push({
            kind: "entry",
            depth: depth,
            card: s.card_id,
            module: f.module,
            qual: s.qualname,
            file: basename(f.path),
            line: s.def_line || 0,
          });
          rows.push({
            kind: "note",
            depth: depth,
            card: s.card_id,
            file: basename(f.path),
          });
        });
        return;
      }
      const open = st().files.has(f.module);
      rows.push({
        kind: "file",
        depth: depth,
        module: f.module,
        path: f.path,
        third: f.third,
        open: open,
      });
      if (open) pushNested(rows, f.module, f.symbols, depth + 1);
    });
}

export function collectItems() {
  const items = [];
  if (store.spView === "entry") {
    const byModule = new Map();
    (store.spEntrySyms || []).forEach(function (s) {
      if (!byModule.has(s.module)) byModule.set(s.module, []);
      byModule.get(s.module).push(s);
    });
    byModule.forEach(function (syms, module) {
      const f = store.spFileByModule.get(module);
      items.push({
        module: module,
        path: (f && f.path) || module.replace(/\./g, "/") + ".py",
        third: !!(f && f.is_third_party),
        symbols: syms,
      });
    });
    return items;
  }
  (store.spFiles || []).forEach(function (f) {
    // 一条定义都没有的文件不进函数树
    if (!f.symbols) return;
    items.push({
      module: f.module,
      path: f.path || f.module,
      third: !!f.is_third_party,
      symbols: store.spCache.get(f.module) || [],
    });
  });
  return items;
}

export function collectRows() {
  const rows = [];
  if (store.spView === "fav" || store.spView === "recent") {
    const ids = store.spView === "fav" ? store.rootData.favorites || [] : store.rootData.recent || [];
    ids.forEach(function (id) {
      const p = splitId(id);
      const f = store.spFileByModule.get(p.module);
      const path = (f && f.path) || p.module.replace(/\./g, "/") + ".py";
      rows.push({
        kind: "entry",
        depth: 0,
        card: id,
        module: p.module,
        qual: p.qualname,
        file: basename(path),
        line: 0,
      });
      // 平铺视图没有目录树给上下文，注释行直接显示整条相对路径
      rows.push({ kind: "note", depth: 0, card: id, file: path });
    });
    return rows;
  }
  store.spTree = buildDirTree(collectItems());
  emitDir(store.spTree.root, "", 0, rows);
  return rows;
}

// 展开与收起用统一的图标这套（同一 viewBox、1.5 线宽、currentColor），
// 只写一个字形的话放大缩小都对不齐别的图标
