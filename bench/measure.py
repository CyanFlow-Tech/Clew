#!/usr/bin/env python3
"""Clew performance baseline.

Reproduces the numbers in ARCHITECTURE.md, section 8. It answers one question:
for the graph of a 500k-line Python project, does parsing or storage need extra
work (GPUs, for instance)?

Usage:
    python measure.py parse  <a directory with many .py files>
    python measure.py sqlite [nodes] [edges]
"""

import ast
import os
import random
import sqlite3
import sys
import tempfile
import time


def measure_parse(root: str) -> None:
    files = []
    for d, _, fs in os.walk(root):
        for f in fs:
            if f.endswith(".py"):
                files.append(os.path.join(d, f))

    lines = 0
    for p in files:
        try:
            with open(p, "rb") as fh:
                lines += fh.read().count(b"\n")
        except OSError:
            pass

    print(f"files: {len(files)}  lines: {lines}")

    t = time.time()
    ok = fail = 0
    for p in files:
        try:
            with open(p, "rb") as fh:
                ast.parse(fh.read())
            ok += 1
        except Exception:
            fail += 1
    elapsed = time.time() - t

    per100k = elapsed / (lines / 100_000) if lines else 0
    print(f"ast.parse: {elapsed:.2f}s  ok={ok} fail={fail}")
    print(f"-> {per100k:.2f}s per 100k LoC  -> 500k LoC ~= {per100k * 5:.2f}s")
    print("note: tree-sitter is expected to be faster; treat this as an upper bound.")


def measure_sqlite(nodes: int, edges: int) -> None:
    p = os.path.join(tempfile.mkdtemp(), "bench.db")
    c = sqlite3.connect(p)
    c.executescript(
        """
        PRAGMA journal_mode=WAL;
        PRAGMA synchronous=NORMAL;
        CREATE TABLE node(id INTEGER PRIMARY KEY, kind INT, name TEXT);
        CREATE TABLE edge(src INT, dst INT, kind INT);
        """
    )

    t = time.time()
    c.executemany("INSERT INTO node VALUES(?,?,?)",
                  ((i, i % 7, "sym_%d" % i) for i in range(nodes)))
    print(f"{nodes//1000}k nodes insert: {time.time()-t:.2f}s")

    t = time.time()
    rnd = random.Random(1)
    c.executemany("INSERT INTO edge VALUES(?,?,?)",
                  ((rnd.randrange(nodes), rnd.randrange(nodes), i % 5) for i in range(edges)))
    print(f"{edges//1000}k edges insert: {time.time()-t:.2f}s")

    t = time.time()
    c.commit()
    print(f"commit: {time.time()-t:.2f}s")

    t = time.time()
    c.execute("CREATE INDEX idx_edge_src ON edge(src)")
    c.execute("CREATE INDEX idx_edge_dst ON edge(dst)")
    c.commit()
    print(f"2 indexes: {time.time()-t:.2f}s")

    t = time.time()
    rounds = 200
    for _ in range(rounds):
        s = rnd.randrange(nodes)
        c.execute("SELECT src,dst FROM edge WHERE src=? OR dst=? LIMIT 50", (s, s)).fetchall()
    print(f"neighborhood query avg: {(time.time()-t)/rounds*1000:.2f} ms")

    print(f"db size: {os.path.getsize(p)/1e6:.1f} MB")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "parse"
    if cmd == "parse":
        root = sys.argv[2] if len(sys.argv) > 2 else "."
        measure_parse(root)
    elif cmd == "sqlite":
        n = int(sys.argv[2]) if len(sys.argv) > 2 else 800_000
        e = int(sys.argv[3]) if len(sys.argv) > 3 else 1_500_000
        measure_sqlite(n, e)
    else:
        print(__doc__)
