use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use blake3::Hasher;
use rayon::prelude::*;

use super::super::model::*;
use crate::utils::text;
use super::types::*;
use super::builder::Builder;
use super::super::progress::weight;
use super::super::walker;

pub fn build_with(
    lang: &dyn walker::Language,
    root: &Path,
    include_venv: bool,
    report: &mut dyn FnMut(&'static str, u64, u64, f32),
) -> anyhow::Result<Built> {
    let t_all = Instant::now();
    let root = std::fs::canonicalize(root)?;

    let t_walk = Instant::now();
    let files = walker::collect_for(lang, &root, include_venv);
    let d_walk = t_walk.elapsed();
    report(
        "枚举文件",
        files.len() as u64,
        files.len() as u64,
        weight::WALK.1,
    );

    // 每个文件互相独立，解析与提取按核并行，之后合并再统一解析调用。
    // 每块 500 个文件报告一次进度，并行期间不至于让进度条停住。
    let d_read = AtomicU64::new(0);
    let d_parse = AtomicU64::new(0);
    let d_emit = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);

    let mut builder = Builder::default();
    builder.symbols_only = true;

    const CHUNK: usize = 500;
    let mut done = 0usize;
    for batch in files.chunks(CHUNK) {
        let parts: Vec<Builder> = batch
            .par_iter()
            .map_init(|| lang.new_parser(), |parser, file| {
                let mut part = Builder::default();
                part.symbols_only = true;

                let t = Instant::now();
                let Ok(source) = std::fs::read_to_string(&file.path) else {
                    return part;
                };
                bytes.fetch_add(source.len() as u64, Ordering::Relaxed);
                d_read.fetch_add(text::nanos(t.elapsed()), Ordering::Relaxed);

                let t = Instant::now();
                let tree = match parser.parse(&source, None) {
                    Some(t) => t,
                    None => return part,
                };
                d_parse.fetch_add(text::nanos(t.elapsed()), Ordering::Relaxed);

                let mut hasher = Hasher::new();
                hasher.update(source.as_bytes());
                let src_hash = hasher.finalize().to_hex().to_string();

                let t = Instant::now();
                part.file(file, source.as_bytes(), tree.root_node(), src_hash);
                d_emit.fetch_add(text::nanos(t.elapsed()), Ordering::Relaxed);
                part
            })
            .collect();

        for part in parts {
            builder.merge(part);
        }
        done += batch.len();
        report(
            "解析源码",
            done as u64,
            files.len() as u64,
            weight::lerp(weight::PARSE, done as u64, files.len() as u64),
        );
    }

    let (d_read, d_parse, d_emit, bytes) = (
        Duration::from_nanos(d_read.load(Ordering::Relaxed)),
        Duration::from_nanos(d_parse.load(Ordering::Relaxed)),
        Duration::from_nanos(d_emit.load(Ordering::Relaxed)),
        bytes.load(Ordering::Relaxed) as usize,
    );

    let t = Instant::now();
    report("解析调用", 0, 1, weight::RESOLVE.0);
    builder.resolve();
    let d_resolve = t.elapsed();
    report("解析调用", 1, 1, weight::RESOLVE.1);
    eprintln!(
        "[t] stages files={} bytes={} walk {:.1}ms read {:.1}ms parse {:.1}ms emit {:.1}ms resolve {:.1}ms total {:.1}ms",
        files.len(),
        bytes,
        d_walk.as_secs_f64() * 1000.0,
        d_read.as_secs_f64() * 1000.0,
        d_parse.as_secs_f64() * 1000.0,
        d_emit.as_secs_f64() * 1000.0,
        d_resolve.as_secs_f64() * 1000.0,
        t_all.elapsed().as_secs_f64() * 1000.0
    );

    let mut imports = Vec::new();
    for (module, table) in &builder.graph.imports {
        for info in table.values() {
            imports.push(ImportMeta {
                module: module.clone(),
                local: info.local.clone(),
                orig: info.orig.clone(),
                source_module: info.module.clone(),
                alias: info.alias,
                module_import: info.module_import,
            });
        }
    }

    Ok(Built {
        graph: Graph {
            cards: builder.graph.cards,
            calls: builder.graph.calls,
        },
        modules: builder.out_modules,
        symbols: builder.out_symbols,
        imports,
        classes: builder.out_classes,
        module_cards: builder.module_cards,
    })
}

