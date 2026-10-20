//! UNIT-rec-format over the committed recordings: every file under
//! `testdata/recordings/<adapter>/<adapter-ver>/<cli-ver>/<scenario>.jsonl`
//! reads as format v1 and sits where its header says.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use cupper_protocol::recording::{INSTANCE_EXITED, Recording, Source, Usage};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read a recordings directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn unit_rec_format_reads_every_committed_recording() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/recordings");
    let mut paths = Vec::new();
    files(&root, &mut paths);
    let mut sp1 = 0;
    for path in &paths {
        let name = path.strip_prefix(&root).expect("under the root");
        let text = fs::read_to_string(path).expect("read a recording");
        let recording = Recording::parse(&text).unwrap_or_else(|e| panic!("{name:?}: {e}"));
        let h = &recording.header;
        let scenario = format!("{}.jsonl", h.scenario);
        let expected = [&h.adapter, &h.adapter_version, &h.cli_version, &scenario];
        assert!(name.iter().eq(expected.map(OsStr::new)), "{name:?}");
        if h.source != Source::Sp1 {
            continue;
        }
        // SP-1: extraction goldens only; one exit per adapter process, and
        // s01 and s09 restart the adapter (once and twice).
        sp1 += 1;
        assert_eq!(h.usage, [Usage::GoldA], "{name:?}");
        let segments: Vec<_> = recording.segments().collect();
        let restarts = match h.scenario.as_str() {
            "s01-init-session" => 1,
            "s09-load-resume" => 2,
            _ => 0,
        };
        assert_eq!(segments.len(), restarts + 1, "{name:?}");
        for segment in segments {
            let exits = segment
                .iter()
                .filter(|f| f.body.event() == Some(INSTANCE_EXITED));
            assert_eq!(exits.count(), 1, "{name:?}");
        }
    }
    assert_eq!(sp1, 48, "the SP-1 recordings");
}
