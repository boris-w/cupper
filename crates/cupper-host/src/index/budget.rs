//! Trust results within one frame (rulings before T034, 5.3.11): the
//! findings and the changes of a result take at most [`BUDGET`] bytes of
//! JSON. Each `detail` is first cut to [`DETAIL_MAX`] bytes on a character
//! boundary, with `…`; then the changes, and after them the findings, go in
//! in order until one does not fit. The result says how many were left out.

use cupper_protocol::methods::project::{Finding, TrustedFileChange};
use serde::Serialize;

/// The bytes of findings and changes in one result; the rest of a 64,000-byte
/// frame is enough for the envelope and the other fields, as for
/// `history/page`.
pub const BUDGET: usize = 48_000;
/// The longest `detail` shown, in bytes.
pub const DETAIL_MAX: usize = 2_000;

#[derive(Debug, PartialEq)]
pub(super) struct Fitted {
    pub findings: Vec<Finding>,
    pub findings_omitted: Option<u32>,
    pub diff: Option<Vec<TrustedFileChange>>,
    pub diff_omitted: Option<u32>,
}

pub(super) fn fit(findings: Vec<Finding>, diff: Option<Vec<TrustedFileChange>>) -> Fitted {
    let mut room = BUDGET;
    let (diff, diff_omitted) = match diff {
        Some(diff) => {
            let (kept, omitted) = take(diff, &mut room);
            (Some(kept), omitted)
        }
        None => (None, None),
    };
    let findings = findings.into_iter().map(|mut finding| {
        cut(&mut finding.detail);
        finding
    });
    let (findings, findings_omitted) = take(findings, &mut room);
    Fitted {
        findings,
        findings_omitted,
        diff,
        diff_omitted,
    }
}

/// The items of `items` that fit `room`, and how many did not.
fn take<T: Serialize>(
    items: impl IntoIterator<Item = T>,
    room: &mut usize,
) -> (Vec<T>, Option<u32>) {
    let (mut kept, mut omitted) = (Vec::new(), 0u32);
    for item in items {
        // The comma between items.
        let size = serde_json::to_vec(&item).map_or(usize::MAX, |json| json.len() + 1);
        if omitted == 0 && size <= *room {
            *room -= size;
            kept.push(item);
        } else {
            omitted += 1;
        }
    }
    (kept, (omitted > 0).then_some(omitted))
}

fn cut(detail: &mut String) {
    if detail.len() <= DETAIL_MAX {
        return;
    }
    let mut end = DETAIL_MAX - '…'.len_utf8();
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail.truncate(end);
    detail.push('…');
}

#[cfg(test)]
mod tests {
    use cupper_protocol::annotations::WirePath;
    use cupper_protocol::item::DiffChange;
    use cupper_protocol::methods::project::FindingKind;

    use super::*;

    fn finding(detail: String) -> Finding {
        Finding {
            kind: FindingKind::Hook,
            path: WirePath(".claude/settings.json".into()),
            target: None,
            outside_workspace: false,
            detail,
        }
    }

    fn change(n: usize) -> TrustedFileChange {
        let path = WirePath(format!(".claude/agents/a{n:04}.md"));
        TrustedFileChange {
            path,
            change: DiffChange::Update,
        }
    }

    #[test]
    fn index_budget_results_fit_and_count_what_is_left_out() {
        // Small results go out whole.
        let small = fit(vec![finding("acme-lint".into())], Some(vec![change(1)]));
        assert_eq!((small.findings_omitted, small.diff_omitted), (None, None));
        assert_eq!(small.findings[0].detail, "acme-lint");

        // A long detail is cut on a character boundary.
        let long = fit(vec![finding("é".repeat(DETAIL_MAX))], None);
        let detail = &long.findings[0].detail;
        assert!(
            detail.len() <= DETAIL_MAX && detail.ends_with('…'),
            "{}",
            detail.len()
        );

        // The changes come first; the findings take what room is left.
        let diff = (0..2_000).map(change).collect();
        let findings = (0..2_000).map(|n| finding(format!("acme-{n}"))).collect();
        let big = fit(findings, Some(diff));
        let kept = big.diff.as_ref().unwrap().len();
        assert_eq!(kept + big.diff_omitted.unwrap() as usize, 2_000);
        assert_eq!(big.findings_omitted, Some(2_000));
        let bytes = serde_json::to_vec(&(&big.findings, &big.diff))
            .unwrap()
            .len();
        assert!(bytes <= BUDGET, "{bytes}");
    }
}
