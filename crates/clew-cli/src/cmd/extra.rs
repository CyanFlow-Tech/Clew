use std::path::PathBuf;


use super::super::{display, ms, open_index};

pub fn export(project: PathBuf, out: PathBuf, db: Option<PathBuf>) -> anyhow::Result<()> {
            use std::io::Write;

            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let snap = clew_core::snapshot::snapshot(&conn, project_id)?;

            if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            let mut file = std::io::BufWriter::new(std::fs::File::create(&out)?);
            serde_json::to_writer(&mut file, &snap)?;
            file.flush()?;
            drop(file);

            let bytes = std::fs::metadata(&out)?.len();
            println!("out        {}", display(&out));
            println!("cards      {}", snap.cards.len());
            println!("calls      {}", snap.calls.len());
            println!("entries    {}", snap.entries.len());
            println!("size       {} KiB", bytes / 1024);
    Ok(())
}

pub fn materialize(project: PathBuf, modules: Vec<String>, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let mut mat = clew_core::scan::materialize::Materializer::open(project_id, conn)?;
            // GUI 打开项目时就建好了，这里对齐一下计时口径
            let t_pre = std::time::Instant::now();
            mat.preload()?;
            println!("preload   {:.1} ms", ms(t_pre.elapsed()));
            let mut total = std::time::Duration::ZERO;
            for module in &modules {
                let t = std::time::Instant::now();
                let cards = mat.ensure(module)?;
                let d = t.elapsed();
                total += d;
                println!("{:<40} {:>4} cards  {:>8.1} ms", module, cards, ms(d));
            }
            if modules.len() > 1 {
                println!(
                    "total {:.1} ms  avg {:.1} ms",
                    ms(total),
                    ms(total) / modules.len() as f64
                );
            }
    Ok(())
}

pub fn bench(project: PathBuf, steps: usize, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;

            // what the GUI does on open: read the entry list, then the first card
            let t = std::time::Instant::now();
            let entries = clew_core::snapshot::entries(&conn, project_id)?;
            let d_entries = t.elapsed();

            let first = entries
                .first()
                .map(|e| e.id.clone())
                .ok_or_else(|| anyhow::anyhow!("no entry points in this project"))?;
            let t = std::time::Instant::now();
            let payload = clew_core::snapshot::cards_of(&conn, project_id, &[first.clone()])?;
            let d_first = t.elapsed();
            let first_calls = payload.calls.len();
            let first_bytes = serde_json::to_string(&payload)?.len();

            // then one expansion per click, following resolved edges breadth first
            let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
            queue.push_back(first);
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut per_step = Vec::new();
            let mut total = std::time::Duration::ZERO;
            let t_all = std::time::Instant::now();
            while per_step.len() < steps {
                let Some(id) = queue.pop_front() else { break };
                if !seen.insert(id.clone()) {
                    continue;
                }
                let t = std::time::Instant::now();
                let payload = clew_core::snapshot::cards_of(&conn, project_id, &[id])?;
                let d = t.elapsed();
                per_step.push(d);
                total += d;
                for call in payload.calls.values() {
                    if let Some(to) = &call.to {
                        if !seen.contains(to) {
                            queue.push_back(to.clone());
                        }
                    }
                }
            }
            let d_all = t_all.elapsed();

            per_step.sort();
            let n = per_step.len();
            let avg = if n > 0 { total.as_secs_f64() / n as f64 } else { 0.0 };
            let min = per_step.first().map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let max = per_step.last().map(|d| d.as_secs_f64()).unwrap_or(0.0);

            println!("entries      {:.2} ms  ({} entry points)", ms(d_entries), entries.len());
            println!(
                "first card   {:.2} ms  ({} call sites, {} bytes)",
                ms(d_first),
                first_calls,
                first_bytes
            );
            println!(
                "open session {:.2} ms  (entries + first card)",
                ms(d_entries + d_first)
            );
            println!("steps        {}", n);
            println!("step total   {:.2} ms", ms(d_all));
            println!(
                "step each    avg {:.2} ms  min {:.2}  max {:.2}",
                avg * 1000.0,
                min * 1000.0,
                max * 1000.0
            );

            // one line of CSV for the plotting script
            println!(
                "csv,{:.3},{:.3},{},{:.3},{:.3},{:.3}",
                ms(d_entries),
                ms(d_first),
                n,
                ms(d_all),
                avg * 1000.0,
                max * 1000.0
            );
    Ok(())
}
