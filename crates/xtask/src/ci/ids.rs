//! Test IDs (`<layer>-<scope>-<name>`, design 5.9.10) and the tests that carry
//! them. A test belongs to an ID when its name, or a module or describe block
//! on its path, starts with the ID: in snake case for Rust (`unit_ci_lock_...`
//! runs `UNIT-ci-lock`), verbatim for JS test titles (`UNIT-shell-mock ...`).
//! The IDs come from the design and the plan, which keep their exact spelling.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Layers that have automated tests. `MAN-` items are manual.
const LAYERS: &[&str] = &[
    "UNIT",
    "PROP",
    "GOLD-A",
    "GOLD-B",
    "IT",
    "E2E",
    "UI",
    "SMOKE-SIGNED",
    "COMPAT",
    "PERF",
];

/// Every test ID mentioned in `docs/design/` and `docs/03-plan.md`.
pub fn harvest(root: &Path) -> Vec<String> {
    let docs = root.join("docs");
    let mut files = vec![docs.join("03-plan.md")];
    if let Ok(entries) = fs::read_dir(docs.join("design")) {
        files.extend(entries.filter_map(|entry| Some(entry.ok()?.path())));
    }
    let mut ids = BTreeSet::new();
    for file in files {
        if let Ok(text) = fs::read_to_string(file) {
            scan(&text, &mut ids);
        }
    }
    ids.into_iter().collect()
}

fn scan(text: &str, ids: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let id_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'-';
    for start in 0..bytes.len() {
        if start > 0 && (id_byte(bytes[start - 1]) || bytes[start - 1] == b'_') {
            continue;
        }
        for layer in LAYERS {
            let rest = &bytes[start..];
            if !(rest.starts_with(layer.as_bytes()) && rest.get(layer.len()) == Some(&b'-')) {
                continue;
            }
            let len = rest.iter().take_while(|b| id_byte(**b)).count();
            let id = String::from_utf8_lossy(&rest[..len]);
            let id = id.trim_end_matches('-');
            if id.len() > layer.len() + 1 {
                ids.insert(id.to_owned());
            }
        }
    }
}

/// The longest ID that starts a `::`-separated segment of `test_name`, either
/// in snake case and followed by `_`, or verbatim and followed by anything
/// that cannot continue an ID.
pub fn owner<'a>(ids: &'a [String], test_name: &str) -> Option<&'a str> {
    let segments: Vec<&str> = test_name.split("::").collect();
    let id_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    ids.iter()
        .filter(|id| {
            let snake = id.to_ascii_lowercase().replace('-', "_");
            segments.iter().any(|segment| {
                let in_snake_case = segment
                    .strip_prefix(snake.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('_'));
                let verbatim = segment
                    .strip_prefix(id.as_str())
                    .is_some_and(|rest| !rest.starts_with(id_char));
                in_snake_case || verbatim
            })
        })
        .max_by_key(|id| id.len())
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_ci_summary_ids_come_from_docs_and_match_test_names() {
        let mut found = BTreeSet::new();
        scan(
            "新增 UNIT-ci-summary、UNIT-ci-lock；IT-NFR9-failpoints-core (`GOLD-A-option-map`). \
             PRD UI-0 is not a test. MAN-S1 is manual; XUNIT-x and my_UNIT-y are not IDs.",
            &mut found,
        );
        let ids: Vec<String> = found.into_iter().collect();
        assert_eq!(
            ids,
            [
                "GOLD-A-option-map",
                "IT-NFR9-failpoints-core",
                "UI-0",
                "UNIT-ci-lock",
                "UNIT-ci-summary",
            ]
        );

        assert_eq!(
            owner(&ids, "ci::lock::tests::unit_ci_lock_refuses_dirty_tree"),
            Some("UNIT-ci-lock")
        );
        assert_eq!(
            owner(&ids, "it_nfr9_failpoints_core::kill_during_commit"),
            Some("IT-NFR9-failpoints-core")
        );
        assert_eq!(owner(&ids, "ci::tests::unit_ci_locked_out"), None);
        assert_eq!(owner(&ids, "ci::tests::helper"), None);

        // JS test titles carry the ID verbatim, in a describe block or the test.
        assert_eq!(
            owner(&ids, "packages/a/src/a.test.ts::UNIT-ci-lock::waits"),
            Some("UNIT-ci-lock")
        );
        assert_eq!(
            owner(&ids, "packages/a/src/a.test.ts::UNIT-ci-summary: no paths"),
            Some("UNIT-ci-summary")
        );
        assert_eq!(
            owner(&ids, "e2e/a.spec.ts::GOLD-A-option-map"),
            Some("GOLD-A-option-map")
        );
        assert_eq!(owner(&ids, "a.test.ts::UNIT-ci-locked waits"), None);
        assert_eq!(owner(&ids, "a.test.ts::UNIT-ci-lock-x waits"), None);
        assert_eq!(owner(&ids, "a.test.ts::unit-ci-lock waits"), None);
        assert_eq!(owner(&ids, "a.test.ts::waits for UNIT-ci-lock"), None);
    }
}
