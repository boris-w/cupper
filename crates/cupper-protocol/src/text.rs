//! Size accounting for text that ends up inside JSON frames.
//!
//! Limits are expressed in JSON-escaped bytes, because escaping can grow a
//! string up to 6x (`\u001f`), and the frame limit applies to the encoded form.

use crate::limits::V1;

/// Largest escaped text of a single `item.delta` written to the log.
pub const MAX_DELTA_ESCAPED: usize = V1.max_delta_bytes as usize;
/// Largest escaped text of a range event produced by send-time coalescing.
pub const MAX_MERGED_ESCAPED: usize = V1.max_merged_bytes as usize;
/// Inline content above this escaped size becomes a blob reference.
pub const MAX_INLINE_ESCAPED: usize = V1.max_inline_bytes as usize;
/// Titles and names are truncated to this many characters.
pub const MAX_LABEL_CHARS: usize = V1.max_label_chars as usize;
/// Preview length for blob-backed content.
pub const PREVIEW_CHARS: usize = 200;

/// Bytes `c` occupies inside a JSON string as serde_json writes it.
pub fn escaped_char_len(c: char) -> usize {
    match c {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' => 2,
        c if (c as u32) < 0x20 => 6,
        c => c.len_utf8(),
    }
}

pub fn escaped_len(s: &str) -> usize {
    s.chars().map(escaped_char_len).sum()
}

/// Split `s` on char boundaries into pieces whose escaped length is at most `max`.
pub fn split_escaped(s: &str, max: usize) -> Vec<&str> {
    assert!(max >= 6, "max must fit any single escaped char");
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut size = 0;
    for (i, c) in s.char_indices() {
        let l = escaped_char_len(c);
        if size + l > max {
            pieces.push(&s[start..i]);
            start = i;
            size = 0;
        }
        size += l;
    }
    if start < s.len() || pieces.is_empty() {
        pieces.push(&s[start..]);
    }
    pieces
}

/// Keep at most `max_chars` characters, marking truncation with an ellipsis.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_owned(),
        Some((i, _)) => format!("{}…", &s[..i]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn unit_proto_base_text_split_pieces_fit_and_concatenate(s in "\\PC{0,2000}|[\\x00-\\x1f\"\\\\a]{0,2000}", max in 6usize..300) {
            let pieces = split_escaped(&s, max);
            prop_assert_eq!(pieces.concat(), s.clone());
            for p in &pieces {
                prop_assert!(escaped_len(p) <= max);
                let encoded = serde_json::to_string(p).unwrap();
                prop_assert_eq!(encoded.len(), escaped_len(p) + 2);
            }
        }
    }
}
