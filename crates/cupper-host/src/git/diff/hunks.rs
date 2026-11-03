//! A patch cut into the units the hunk operations act on (design 5.2.10,
//! ADR-10): each `@@` hunk; a file's mode change, a unit of its own; and a
//! whole file where hunks mean nothing: added, deleted, binary, symlink,
//! submodule and LFS files. A unit's ID is a hash of its file and its
//! content, so the same change has the same ID in every diff.

use cupper_protocol::methods::git::{DiffHunk, HunkUnit};
use sha2::{Digest, Sha256};

/// The units of a `diff-tree -p --no-renames` patch, file after file. Every
/// unit has `lines`; the caller moves long ones into blobs.
pub fn units(patch: &[u8]) -> Vec<DiffHunk> {
    let mut lines: Vec<&[u8]> = patch.split(|b| *b == b'\n').collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let starts = (0..lines.len()).filter(|&i| lines[i].starts_with(b"diff --git "));
    let mut starts: Vec<usize> = starts.collect();
    starts.push(lines.len());
    let files = starts.windows(2).map(|w| &lines[w[0]..w[1]]);
    files.flat_map(file).collect()
}

/// The units of one file's part of the patch.
fn file(lines: &[&[u8]]) -> Vec<DiffHunk> {
    let body_at = lines
        .iter()
        .position(|line| line.starts_with(b"@@ ") || line.starts_with(b"GIT binary patch"));
    let (head, body) = lines.split_at(body_at.unwrap_or(lines.len()));
    let field = |prefix: &[u8]| head.iter().find_map(|line| line.strip_prefix(prefix));
    let modes = [
        "old mode ",
        "new mode ",
        "new file mode ",
        "deleted file mode ",
    ];
    let mut modes: Vec<&[u8]> = modes.iter().filter_map(|m| field(m.as_bytes())).collect();
    // `index <old>..<new> <mode>` when the mode stays.
    let index = field(b"index ").and_then(|i| i.rsplit(|b| *b == b' ').next());
    modes.extend(index.filter(|mode| !mode.contains(&b'.')));
    let binary = body
        .first()
        .is_some_and(|l| l.starts_with(b"GIT binary patch"));
    let lfs = body
        .iter()
        .any(|line| line.get(1..).is_some_and(|l| l.starts_with(LFS)));
    let whole = field(b"new file mode ").is_some()
        || field(b"deleted file mode ").is_some()
        || binary
        || modes.iter().any(|m| *m == b"120000" || *m == b"160000")
        || lfs;
    let diff = lines[0];
    if whole {
        let mut header: Vec<&[u8]> = head[1..]
            .iter()
            .copied()
            .filter(|l| !is_file_line(l))
            .collect();
        let (hunk_heads, rest): (Vec<&[u8]>, Vec<&[u8]>) =
            body.iter().partition(|line| line.starts_with(b"@@ "));
        header.extend(if binary { &body[..1] } else { &hunk_heads[..] });
        let shown = (!binary).then_some(&rest[..]);
        return vec![unit(diff, HunkUnit::WholeFile, lines, &header, shown)];
    }
    let mut units = Vec::new();
    if let (Some(old), Some(new)) = (field(b"old mode "), field(b"new mode ")) {
        let header = [[b"old mode ", old].concat(), [b"new mode ", new].concat()];
        let header: Vec<&[u8]> = header.iter().map(Vec::as_slice).collect();
        units.push(unit(diff, HunkUnit::Mode, &header, &header, None));
    }
    let mut at = 0;
    while at < body.len() {
        let end = (at + 1..body.len()).find(|&i| body[i].starts_with(b"@@ "));
        let end = end.unwrap_or(body.len());
        let hunk = &body[at..end];
        units.push(unit(
            diff,
            HunkUnit::Hunk,
            hunk,
            &hunk[..1],
            Some(&hunk[1..]),
        ));
        at = end;
    }
    units
}

/// The start of an LFS pointer file's first line.
const LFS: &[u8] = b"version https://git-lfs.github.com/spec/";

/// The lines a unit's header leaves out: they name the file, not the change.
fn is_file_line(line: &[u8]) -> bool {
    [&b"index "[..], b"--- ", b"+++ "]
        .iter()
        .any(|p| line.starts_with(p))
}

/// A unit, its ID the hash of the file's `diff --git` line, its kind and
/// `content`.
fn unit(
    diff: &[u8],
    kind: HunkUnit,
    content: &[&[u8]],
    header: &[&[u8]],
    lines: Option<&[&[u8]]>,
) -> DiffHunk {
    let mut hash = Sha256::new();
    hash.update(diff);
    hash.update([0]);
    hash.update(kind.as_str());
    for line in content {
        hash.update([b'\n']);
        hash.update(line);
    }
    let hex: String = hash.finalize()[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let text = |line: &&[u8]| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        String::from_utf8_lossy(line).into_owned()
    };
    let header: Vec<String> = header.iter().map(text).collect();
    DiffHunk {
        hunk_id: format!("hk_{hex}").parse().expect("hk_ and hex digits"),
        unit: kind,
        header: header.join("\n"),
        lines: lines.map(|lines| lines.iter().map(text).collect()),
        lines_blob: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(units: &[DiffHunk]) -> Vec<(String, String, usize)> {
        let lines = |u: &DiffHunk| u.lines.as_ref().map_or(0, Vec::len);
        let row = |u: &DiffHunk| (u.unit.as_str().to_owned(), u.header.clone(), lines(u));
        units.iter().map(row).collect()
    }

    const MODE_AND_HUNKS: &str = "diff --git a/f b/f\nold mode 100644\nnew mode 100755\n\
        index 0ff3bbb..9bebd18\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n 1\n-2\n+two\n 3\n\
        @@ -15,3 +15,3 @@ fn x\n 15\n-16\n+sixteen\n\\ No newline at end of file\n";

    #[test]
    fn unit_git_hunks_a_file_is_its_mode_and_its_hunks() {
        let cut = units(MODE_AND_HUNKS.as_bytes());
        let mode = "old mode 100644\nnew mode 100755";
        assert_eq!(
            shown(&cut),
            [
                ("mode".into(), mode.into(), 0),
                ("hunk".into(), "@@ -1,3 +1,3 @@".into(), 4),
                ("hunk".into(), "@@ -15,3 +15,3 @@ fn x".into(), 4),
            ]
        );
        assert_eq!(
            cut[2].lines.as_ref().unwrap()[3],
            "\\ No newline at end of file"
        );
        // Addressed by content: the same hunk in another diff, after other
        // changes to the file, has the same ID; another file's does not.
        let alone = "diff --git a/f b/f\nindex 1111111..2222222 100644\n--- a/f\n+++ b/f\n\
            @@ -15,3 +15,3 @@ fn x\n 15\n-16\n+sixteen\n\\ No newline at end of file\n";
        assert_eq!(units(alone.as_bytes())[0].hunk_id, cut[2].hunk_id);
        let other = alone.replace("a/f b/f", "a/g b/g");
        assert_ne!(units(other.as_bytes())[0].hunk_id, cut[2].hunk_id);
        let ids: std::collections::BTreeSet<_> = cut.iter().map(|u| &u.hunk_id).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn unit_git_hunks_whole_files_are_one_unit() {
        let patch = "diff --git a/new b/new\nnew file mode 100644\nindex 0000000..975fbec\n\
            --- /dev/null\n+++ b/new\n@@ -0,0 +1,2 @@\n+y\r\n+z\r\n\
            diff --git a/bin b/bin\nindex bdc955b..8835708 100644\nGIT binary patch\nliteral 2\n\
            JcmZQz0ssI600RI3\n\nliteral 2\nJcmZQz1ONa700IC2\n\n\
            diff --git a/link b/link\nindex 1de5659..27fa349 120000\n--- a/link\n+++ b/link\n\
            @@ -1 +1 @@\n-target\n\\ No newline at end of file\n+other\n\\ No newline at end of file\n\
            diff --git a/sub b/sub\nindex 1111111..2222222 160000\n--- a/sub\n+++ b/sub\n\
            @@ -1 +1 @@\n-Subproject commit 1111111\n+Subproject commit 2222222\n\
            diff --git a/big.bin b/big.bin\nindex 3333333..4444444 100644\n--- a/big.bin\n\
            +++ b/big.bin\n@@ -1,3 +1,3 @@\n version https://git-lfs.github.com/spec/v1\n\
            -oid sha256:aa\n+oid sha256:bb\n size 12\n\
            diff --git a/gone b/gone\ndeleted file mode 100755\nindex 5555555..0000000\n\
            --- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
        let cut = units(patch.as_bytes());
        assert!(cut.iter().all(|u| u.unit == HunkUnit::WholeFile));
        let headers: Vec<&str> = cut.iter().map(|u| u.header.as_str()).collect();
        assert_eq!(
            headers,
            [
                "new file mode 100644\n@@ -0,0 +1,2 @@",
                "GIT binary patch",
                "@@ -1 +1 @@",
                "@@ -1 +1 @@",
                "@@ -1,3 +1,3 @@",
                "deleted file mode 100755\n@@ -1 +0,0 @@",
            ]
        );
        // Line ends go, CRLF too; a binary file has no lines to show.
        assert_eq!(cut[0].lines, Some(vec!["+y".into(), "+z".into()]));
        assert_eq!(cut[1].lines, None);
        assert_eq!(cut[2].lines.as_ref().map(Vec::len), Some(4));
    }
}
