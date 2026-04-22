//! Emoji shortcode replacement filter.
//!
//! When enabled, walks the event stream and replaces gemoji-style
//! `:shortcode:` sequences in `Event::Text` payloads with the corresponding
//! emoji character. Lookups use the `emojis` crate's embedded gemoji
//! database.
//!
//! Shortcodes inside fenced code blocks are preserved. Inline code
//! spans (`Event::Code`) are also preserved because we only transform
//! `Event::Text` events. Unknown shortcodes are left as literal text.

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

/// Apply emoji shortcode replacement to a full event stream in place.
///
/// Tracks code-block nesting depth so shortcodes inside fenced code blocks
/// are preserved. Inline code (`Event::Code`) is passed through untouched
/// because we only scan `Event::Text` events.
pub fn replace(events: &mut Vec<Event<'_>>) {
    let mut code_depth: usize = 0;

    for i in 0..events.len() {
        match &events[i] {
            Event::Start(Tag::CodeBlock(_)) => {
                code_depth += 1;
                continue;
            }
            Event::End(TagEnd::CodeBlock) => {
                code_depth = code_depth.saturating_sub(1);
                continue;
            }
            Event::Text(_) if code_depth == 0 => {}
            _ => continue,
        }

        // Take ownership of the text so we can feed it to `replace_shortcodes`
        // and emit a new Text event with the result.
        if let Event::Text(text) = std::mem::replace(&mut events[i], Event::SoftBreak) {
            match replace_shortcodes(&text) {
                Some(replaced) => {
                    events[i] = Event::Text(CowStr::Boxed(replaced.into_boxed_str()));
                }
                None => {
                    events[i] = Event::Text(text);
                }
            }
        }
    }
}

/// Scan `text` for `:shortcode:` patterns and replace each match with its
/// emoji character. Returns `None` when no replacements were made so the
/// caller can skip rebuilding the event.
fn replace_shortcodes(text: &str) -> Option<String> {
    // Fast path: if there's no colon at all, there's nothing to replace.
    // This is the common case for most text runs.
    if !text.contains(':') {
        return None;
    }

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    // `last_emit` points at the first byte we haven't copied into `out` yet.
    // `cursor` is the scanning position, which can run ahead of `last_emit`
    // across unmatched `:` candidates without losing the intermediate text.
    let mut last_emit = 0usize;
    let mut cursor = 0usize;
    let mut replaced_any = false;

    while let Some(rel) = text[cursor..].find(':') {
        let open = cursor + rel;

        // Look for the closing colon on the same run. The shortcode body
        // must be non-empty and only contain `[a-z0-9_+-]`. If we hit an
        // invalid char before a closing colon, the whole range is not a
        // shortcode and we continue scanning from just past this open colon.
        let mut close = None;
        let mut scan = open + 1;
        while scan < bytes.len() {
            let b = bytes[scan];
            if b == b':' {
                close = Some(scan);
                break;
            }
            let valid =
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'+' || b == b'-';
            if !valid {
                break;
            }
            scan += 1;
        }

        if let Some(close_idx) = close {
            if close_idx > open + 1 {
                let name = &text[open + 1..close_idx];
                if let Some(emoji) = emojis::get_by_shortcode(name) {
                    // Flush the literal run between the last emitted
                    // position and this match's open colon, then emit the
                    // emoji character in place of the full `:name:` span.
                    out.push_str(&text[last_emit..open]);
                    out.push_str(emoji.as_str());
                    last_emit = close_idx + 1;
                    cursor = close_idx + 1;
                    replaced_any = true;
                    continue;
                }
            }
        }

        // Not a match (no closing colon, empty name, invalid char, or
        // unknown shortcode).
        cursor = open + 1;
    }

    if !replaced_any {
        return None;
    }

    // Flush the tail after the last successful match.
    out.push_str(&text[last_emit..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{replace, replace_shortcodes};
    use pulldown_cmark::{CowStr, Event};

    #[test]
    fn basic_replacement() {
        assert_eq!(
            replace_shortcodes("Ship it! :rocket:").as_deref(),
            Some("Ship it! 🚀")
        );
    }

    #[test]
    fn multiple_in_one_string() {
        assert_eq!(
            replace_shortcodes(":tada: :rocket: :100:").as_deref(),
            Some("🎉 🚀 💯")
        );
    }

    #[test]
    fn adjacent_shortcodes() {
        assert_eq!(
            replace_shortcodes(":rocket::tada:").as_deref(),
            Some("🚀🎉")
        );
    }

    #[test]
    fn unknown_shortcode_left_as_is() {
        assert_eq!(replace_shortcodes(":not_a_real_emoji:"), None);
        assert_eq!(
            replace_shortcodes(":rocket: and :not_a_real_emoji:").as_deref(),
            Some("🚀 and :not_a_real_emoji:")
        );
    }

    #[test]
    fn fast_path_no_colon() {
        assert_eq!(replace_shortcodes("nothing to see here"), None);
    }

    #[test]
    fn case_sensitive_lowercase_only() {
        // gemoji shortcodes are canonical lowercase—:Rocket: doesn't match.
        assert_eq!(replace_shortcodes(":Rocket:"), None);
    }

    #[test]
    fn bare_colons_unchanged() {
        assert_eq!(replace_shortcodes("8:00:00 am"), None);
        assert_eq!(replace_shortcodes("foo:bar"), None);
        assert_eq!(replace_shortcodes(":"), None);
        assert_eq!(replace_shortcodes("::"), None);
    }

    #[test]
    fn hyphen_and_underscore_in_names() {
        // gemoji uses both. `+1` / `-1` are valid thumbs-up/down.
        assert_eq!(replace_shortcodes(":+1:").as_deref(), Some("👍"));
        assert_eq!(replace_shortcodes(":-1:").as_deref(), Some("👎"));
    }

    #[test]
    fn replace_transforms_rocket_shortcode_in_event_stream() {
        let mut events = vec![Event::Text(CowStr::Borrowed(":rocket:"))];
        replace(&mut events);
        match &events[0] {
            Event::Text(t) => assert_eq!(t.as_ref(), "🚀"),
            other => panic!("expected Text event, got {other:?}"),
        }
    }
}
