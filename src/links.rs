//! Clickable parts of plain text (descriptions, comments): web links, timestamps, #hashtags
//! and @handles.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Url(String),
    /// A position in the video, in seconds.
    Time(f64),
    /// `#tag`, with the `#`.
    Tag(String),
    /// `@handle` of a channel, with the `@`.
    Handle(String),
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
        let before = text[..i].chars().next_back();
        if before.is_none_or(|c| !c.is_alphanumeric() && c != ':' && c != '.') {
            if let Some((len, secs)) = timestamp(rest) {
                out.push((i..i + len, Link::Time(secs)));
                i += len;
                continue;
            }
        }
        // Not inside a word, an e-mail address or an HTML entity (`&#39;`).
        if before.is_none_or(|c| !c.is_alphanumeric() && !"&/_@#".contains(c)) {
            if let Some(len) = tag(rest) {
                out.push((i..i + len, Link::Tag(rest[..len].to_string())));
                i += len;
                continue;
            }
            if let Some(len) = handle(rest) {
                out.push((i..i + len, Link::Handle(rest[..len].to_string())));
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

/// `#` and letters, digits or `_`, not only digits (`#1` is no hashtag): its length.
fn tag(s: &str) -> Option<usize> {
    let body = s.strip_prefix('#')?;
    let len: usize = body.chars().take_while(|c| c.is_alphanumeric() || *c == '_').map(char::len_utf8).sum();
    body[..len].chars().any(|c| !c.is_ascii_digit()).then_some(1 + len)
}

/// `@` and a YouTube handle (3–30 of letters, digits, `.`, `_`, `-`; not ending in `.` or `-`):
/// its length.
fn handle(s: &str) -> Option<usize> {
    let body = s.strip_prefix('@')?;
    let mut len = body.chars().take_while(|c| c.is_ascii_alphanumeric() || "._-".contains(*c)).count();
    while len > 0 && ".-".contains(&body[len - 1..len]) {
        len -= 1;
    }
    ((3..=30).contains(&len) && !body[len..].starts_with('@')).then_some(1 + len)
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
    fn tags_and_handles() {
        assert_eq!(
            links("New video #rust #GameDev_2 by @SomeOne-x."),
            [("#rust", Link::Tag("#rust".into())), ("#GameDev_2", Link::Tag("#GameDev_2".into())), ("@SomeOne-x", Link::Handle("@SomeOne-x".into()))]
        );
        assert_eq!(links("#игры"), [("#игры", Link::Tag("#игры".into()))]);
        // Not tags or handles: numbers, the middle of a word, an entity, an e-mail, a link's anchor.
        assert!(links("#1 a#b &#39; me@mail.com @ab https://x.org/p#top").iter().all(|(_, l)| matches!(l, Link::Url(_))));
    }

    #[test]
    fn non_ascii_text() {
        assert_eq!(links("Глава 2 — 5:07 тут"), [("5:07", Link::Time(307.))]);
    }
}
