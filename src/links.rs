//! Clickable parts of plain text (descriptions, comments): web links and timestamps.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Url(String),
    /// A position in the video, in seconds.
    Time(f64),
}

/// The links in `text`, in order, with their byte ranges.
pub fn find(text: &str) -> Vec<(Range<usize>, Link)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if rest.starts_with("https://") || rest.starts_with("http://") {
            let len = url_len(rest);
            out.push((i..i + len, Link::Url(rest[..len].to_string())));
            i += len;
            continue;
        }
        let after_boundary = text[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != ':' && c != '.');
        if after_boundary {
            if let Some((len, secs)) = timestamp(rest) {
                out.push((i..i + len, Link::Time(secs)));
                i += len;
                continue;
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// A link runs to the next space; punctuation that ends a sentence, and a `)` it didn't open,
/// stay outside.
fn url_len(s: &str) -> usize {
    let mut end = s.find(char::is_whitespace).unwrap_or(s.len());
    loop {
        let url = &s[..end];
        let Some(last) = url.chars().next_back() else { break };
        let unopened = last == ')' && url.matches('(').count() < url.matches(')').count();
        if ".,;:!?'\"]}>".contains(last) || unopened {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    end
}

/// `M:SS`, `MM:SS` or `H:MM:SS` at the start of `s`, not followed by more digits, letters or
/// another `:`: its length and seconds.
fn timestamp(s: &str) -> Option<(usize, f64)> {
    let b = s.as_bytes();
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut parts = Vec::new();
    let mut at = 0;
    loop {
        let n = digits(at);
        // The first part has 1–2 digits, the others exactly 2.
        if n == 0 || n > 2 || (!parts.is_empty() && n != 2) {
            return None;
        }
        parts.push(s[at..at + n].parse::<u32>().ok()?);
        at += n;
        if parts.len() < 3 && b.get(at) == Some(&b':') && b.get(at + 1).is_some_and(u8::is_ascii_digit) {
            at += 1;
        } else {
            break;
        }
    }
    if parts.len() < 2 || s[at..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == ':') {
        return None;
    }
    let (h, m, sec) = match parts[..] {
        [m, sec] => (0, m, sec),
        [h, m, sec] => (h, m, sec),
        _ => return None,
    };
    if sec >= 60 || (parts.len() == 3 && m >= 60) {
        return None;
    }
    Some((at, f64::from(h * 3600 + m * 60 + sec)))
}

#[cfg(test)]
mod tests {
    use super::{Link, find};

    fn links(text: &str) -> Vec<(&str, Link)> {
        find(text).into_iter().map(|(r, l)| (&text[r], l)).collect()
    }

    #[test]
    fn timestamps() {
        assert_eq!(links("0:00 Intro\n12:34 Boss"), [("0:00", Link::Time(0.)), ("12:34", Link::Time(754.))]);
        assert_eq!(links("at 1:02:03, then"), [("1:02:03", Link::Time(3723.))]);
        assert_eq!(links("(3:45) lol"), [("3:45", Link::Time(225.))]);
    }

    #[test]
    fn not_timestamps() {
        assert!(links("16:9 screen, 4:30pm, 1:2, 10:75, v1.2:30, 123:45, 1:02:03:04").is_empty());
    }

    #[test]
    fn urls() {
        assert_eq!(links("see https://example.com/a?b=1."), [("https://example.com/a?b=1", Link::Url("https://example.com/a?b=1".into()))]);
        assert_eq!(links("(http://x.org/wiki/A_(b))"), [("http://x.org/wiki/A_(b)", Link::Url("http://x.org/wiki/A_(b)".into()))]);
        // A time inside a link is part of the link.
        assert_eq!(links("https://youtu.be/x?t=1:23 ok").len(), 1);
    }

    #[test]
    fn non_ascii_text() {
        assert_eq!(links("Глава 2 — 5:07 тут"), [("5:07", Link::Time(307.))]);
    }
}
