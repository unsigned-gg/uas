// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Token multiplexer for streamed completions (spec 0017 D4).
//!
//! `ThinkSplitter` separates `<think>` blocks from clean text in a chunked stream
//! without heap allocation (D6).

/// Where the stream stands relative to the think block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// No tag seen yet.
    Initial,
    /// Inside a `<think>` block.
    Thinking,
    /// After a `</think>` that closed a block.
    Completion,
}

/// A slice of the stream, labelled by where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece<'a> {
    /// Text for the client.
    Clean(&'a str),
    /// Text for telemetry.
    Thought(&'a str),
}

/// Longest held prefix: `len("</think>") - 1`.
const WINDOW: usize = 7;

const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";
const TAGS: [&str; 2] = [OPEN, CLOSE];

/// Splits think blocks from clean text across chunk boundaries.
#[derive(Debug)]
pub struct ThinkSplitter {
    phase: Phase,
    window: [u8; WINDOW],
    len: usize,
}

impl Default for ThinkSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkSplitter {
    /// A splitter at the start of a stream.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            phase: Phase::Initial,
            window: [0; WINDOW],
            len: 0,
        }
    }

    /// The current phase.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// The held tag prefix.
    #[must_use]
    pub fn held(&self) -> &str {
        std::str::from_utf8(&self.window[..self.len]).unwrap_or_default()
    }

    /// Feed one chunk. Text is emitted as it is classified; a trailing tag prefix is held.
    pub fn push(&mut self, text: &str, emit: &mut impl FnMut(Piece<'_>)) {
        let mut rest = text;
        // Resolve held bytes against the head of this chunk. Held and consumed bytes are ASCII.
        while self.len > 0 {
            let Some(&b) = rest.as_bytes().first() else {
                return;
            };
            let n = self.len;
            let Some(tag) = TAGS
                .iter()
                .find(|t| t.as_bytes()[..n] == self.window[..n] && t.as_bytes().get(n) == Some(&b))
            else {
                self.flush_held(emit);
                break;
            };
            rest = &rest[1..];
            if tag.len() == n + 1 {
                self.len = 0;
                self.apply(tag);
            } else {
                self.window[n] = b;
                self.len = n + 1;
            }
        }
        let mut start = 0;
        let mut from = 0;
        while let Some(off) = rest[from..].find('<') {
            let at = from + off;
            let tail = &rest[at..];
            if let Some(tag) = TAGS.iter().find(|t| tail.starts_with(**t)) {
                self.emit_text(&rest[start..at], emit);
                self.apply(tag);
                start = at + tag.len();
                from = start;
            } else if TAGS.iter().any(|t| t.starts_with(tail)) {
                self.emit_text(&rest[start..at], emit);
                self.window[..tail.len()].copy_from_slice(tail.as_bytes());
                self.len = tail.len();
                return;
            } else {
                from = at + 1;
            }
        }
        self.emit_text(&rest[start..], emit);
    }

    /// End of stream: emit held bytes in the phase they were read in, and reset.
    pub fn finish(&mut self, emit: &mut impl FnMut(Piece<'_>)) {
        self.flush_held(emit);
        self.phase = Phase::Initial;
    }

    fn emit_text(&self, text: &str, emit: &mut impl FnMut(Piece<'_>)) {
        if text.is_empty() {
            return;
        }
        emit(if self.phase == Phase::Thinking {
            Piece::Thought(text)
        } else {
            Piece::Clean(text)
        });
    }

    fn flush_held(&mut self, emit: &mut impl FnMut(Piece<'_>)) {
        let n = std::mem::take(&mut self.len);
        if let Ok(held) = std::str::from_utf8(&self.window[..n]) {
            self.emit_text(held, emit);
        }
    }

    fn apply(&mut self, tag: &str) {
        if tag == OPEN {
            self.phase = Phase::Thinking;
        } else if self.phase == Phase::Thinking {
            self.phase = Phase::Completion;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&str]) -> (String, String) {
        let mut s = ThinkSplitter::new();
        let (mut clean, mut thought) = (String::new(), String::new());
        let mut emit = |p: Piece<'_>| match p {
            Piece::Clean(t) => clean.push_str(t),
            Piece::Thought(t) => thought.push_str(t),
        };
        for c in chunks {
            s.push(c, &mut emit);
        }
        s.finish(&mut emit);
        (clean, thought)
    }

    fn pair(c: &str, t: &str) -> (String, String) {
        (c.to_owned(), t.to_owned())
    }

    #[test]
    fn whole_tags_in_one_push() {
        assert_eq!(run(&["a<think>b</think>c"]), pair("ac", "b"));
    }

    #[test]
    fn every_split_across_two_pushes() {
        let text = "a<think>b</think>c";
        for i in 0..=text.len() {
            let (x, y) = text.split_at(i);
            assert_eq!(run(&[x, y]), pair("ac", "b"), "split at {i}");
        }
    }

    #[test]
    fn every_split_across_three_pushes() {
        let text = "a<think>b</think>c";
        for i in 0..=text.len() {
            for j in i..=text.len() {
                let (x, rest) = text.split_at(i);
                let (y, z) = rest.split_at(j - i);
                assert_eq!(run(&[x, y, z]), pair("ac", "b"), "splits at {i},{j}");
            }
        }
    }

    #[test]
    fn every_split_across_one_byte_pushes() {
        let text = "a<think>b</think>c";
        let chunks: Vec<&str> = (0..text.len()).map(|i| &text[i..=i]).collect();
        assert_eq!(run(&chunks), pair("ac", "b"));
    }

    #[test]
    fn lone_angle_bracket_at_end_is_clean() {
        assert_eq!(run(&["a<"]), pair("a<", ""));
    }

    #[test]
    fn partial_open_tag_at_end_is_clean() {
        assert_eq!(run(&["x<thin"]), pair("x<thin", ""));
    }

    #[test]
    fn held_bytes_that_were_not_a_tag_are_emitted_first() {
        assert_eq!(
            run(&["a<thin", "g is <b>fine"]),
            pair("a<thing is <b>fine", "")
        );
        assert_eq!(run(&["a</thin", "k x"]), pair("a</think x", ""));
    }

    #[test]
    fn unclosed_think_is_thought_never_clean() {
        assert_eq!(run(&["a<think>b"]), pair("a", "b"));
        assert_eq!(run(&["<think>b</thin"]), pair("", "b</thin"));
        assert_eq!(run(&["<think>b<"]), pair("", "b<"));
    }

    #[test]
    fn stray_close_tag_is_dropped() {
        assert_eq!(run(&["a</think>b"]), pair("ab", ""));
        assert_eq!(run(&["a</thin", "k>b"]), pair("ab", ""));
        assert_eq!(run(&["<think>x</think>y</think>z"]), pair("yz", "x"));
    }

    #[test]
    fn two_think_blocks() {
        assert_eq!(
            run(&["a<think>b</think>c<think>d</think>e"]),
            pair("ace", "bd")
        );
    }

    #[test]
    fn nested_open_tag_stays_thinking() {
        assert_eq!(run(&["<think>a<think>b</think>c"]), pair("c", "ab"));
    }

    #[test]
    fn multibyte_text_around_a_tag() {
        let (x, y) = ("é<think>日本</think>ü", "é<think>日");
        assert_eq!(run(&[x]), pair("éü", "日本"));
        let rest = &x[y.len()..];
        assert_eq!(run(&[y, rest]), pair("éü", "日本"));
        assert_eq!(run(&["é<", "日"]), pair("é<日", ""));
    }

    #[test]
    fn empty_push_changes_nothing() {
        assert_eq!(
            run(&["", "a<th", "", "ink>b", "", "</think>c", ""]),
            pair("ac", "b")
        );
        assert_eq!(run(&[]), pair("", ""));
    }

    #[test]
    fn window_holds_ascii_only_and_is_valid_str() {
        let mut s = ThinkSplitter::new();
        s.push("é</thin", &mut |_| {});
        assert_eq!(s.held(), "</thin");
        s.push("é", &mut |_| {});
        assert_eq!(s.held(), "");
    }

    #[test]
    fn finish_resets_the_splitter() {
        let mut s = ThinkSplitter::new();
        s.push("<think>a<", &mut |_| {});
        s.finish(&mut |_| {});
        assert_eq!(s.phase(), Phase::Initial);
        assert_eq!(s.held(), "");
    }
}
