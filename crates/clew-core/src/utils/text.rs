use std::time::Duration;

use tree_sitter::Node;

use crate::card::CardId;

const LINE_LIMIT: usize = 96;

pub(crate) fn of(src: &[u8], node: Node) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

pub(crate) fn clipped(raw: &str) -> String {
    let line = raw.lines().next().unwrap_or("").trim();
    if line.chars().count() > LINE_LIMIT {
        let mut s: String = line.chars().take(LINE_LIMIT - 1).collect();
        s.push('…');
        s
    } else {
        line.to_string()
    }
}

pub(crate) fn first_line(src: &[u8], node: Node) -> String {
    clipped(&of(src, node))
}

pub(crate) fn replacing(
    src: &[u8],
    start: usize,
    end: usize,
    mut repls: Vec<(usize, usize, String)>,
) -> String {
    repls.sort_by_key(|(s, e, _)| (*s, std::cmp::Reverse(*e)));
    let mut out = String::new();
    let mut cursor = start;
    for (s, e, name) in repls {
        if s < cursor || e > end {
            continue;
        }
        out.push_str(&String::from_utf8_lossy(&src[cursor..s]));
        out.push_str(&name);
        cursor = e;
    }
    out.push_str(&String::from_utf8_lossy(&src[cursor..end]));
    out
}

pub(crate) fn short_name(card_id: &str) -> String {
    let qualname = CardId::parse(card_id)
        .map(|id| id.qualname().to_string())
        .unwrap_or_else(|| card_id.to_string());
    qualname.rsplit('.').next().unwrap_or(&qualname).to_string()
}

pub(crate) fn nanos(d: Duration) -> u64 {
    d.as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_keeps_the_first_line_and_marks_the_cut() {
        assert_eq!(clipped("  x = 1  "), "x = 1");
        assert_eq!(clipped("a\nb"), "a");
        let long = "x".repeat(LINE_LIMIT + 10);
        let cut = clipped(&long);
        assert_eq!(cut.chars().count(), LINE_LIMIT);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn the_outer_span_wins_when_two_spans_share_a_start() {
        let src = b"a.b.c()";
        let repls = vec![
            (0, 3, "%inner".to_string()),
            (0, 7, "%outer".to_string()),
        ];
        assert_eq!(replacing(src, 0, 7, repls), "%outer");
    }

    #[test]
    fn replacement_order_is_stable_for_equal_starts() {
        let src = b"a.b.c()";
        let forward = vec![
            (0, 3, "%inner".to_string()),
            (0, 7, "%outer".to_string()),
        ];
        let backward = vec![
            (0, 7, "%outer".to_string()),
            (0, 3, "%inner".to_string()),
        ];
        assert_eq!(
            replacing(src, 0, 7, forward),
            replacing(src, 0, 7, backward)
        );
    }

    #[test]
    fn short_name_takes_the_last_segment() {
        assert_eq!(short_name("pkg.mod:Outer.inner"), "inner");
        assert_eq!(short_name("no-colon.here"), "here");
    }
}
