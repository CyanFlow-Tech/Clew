
use clew_core::walk::{EdgeView, NodeView};


pub(crate) fn enterable(edge: &EdgeView) -> bool {
    !edge.is_third_party
}

/// Bare name, plus the module when it differs from the caller's.
pub(crate) fn label(caller: &NodeView, edge: &EdgeView) -> String {
    if edge.module == caller.module {
        edge.short().to_string()
    } else {
        format!("{}  ·{}", edge.qualname, edge.module)
    }
}

/// `×3  L141, L146, L150`
pub(crate) fn sites(edge: &EdgeView) -> String {
    let mut lns = Vec::new();
    for hit in &edge.hits {
        let tag = format!("L{}", hit.ln);
        if !lns.contains(&tag) {
            lns.push(tag);
        }
    }
    let shown = if lns.len() > 3 {
        format!("{}, …", lns[..3].join(", "))
    } else {
        lns.join(", ")
    };
    if edge.hits.len() > 1 {
        format!("×{}  {shown}", edge.hits.len())
    } else {
        shown
    }
}

/// `  ⚠ 候选 A / B` when dispatch stayed dynamic.
pub(crate) fn poly_note(edge: &EdgeView) -> Option<String> {
    let hit = edge.hits.iter().find(|h| !h.candidates.is_empty())?;
    let names: Vec<&str> = hit
        .candidates
        .iter()
        .map(|c| c.rsplit('.').next().unwrap_or(c))
        .collect();
    Some(format!("  ⚠ 候选 {}", names.join(" / ")))
}
