//! The editing model behind text fields.
//!
//! [`TextEditor`] owns a string, a caret and a selection anchor (both byte
//! offsets on grapheme boundaries) and an undo history. It knows nothing about
//! layout or rendering: elements map pointer positions to offsets through
//! `TextLayout` and call into the editor, which keeps every operation testable
//! without a window.

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const MAX_HISTORY: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Insert,
    Delete,
    Other,
}

#[derive(Clone, Debug)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}

/// Text with a caret, selection and undo/redo. Single-line by default;
/// [`TextEditor::set_multiline`] keeps line breaks and tabs.
#[derive(Clone, Debug, Default)]
pub struct TextEditor {
    text: String,
    cursor: usize,
    anchor: usize,
    max_chars: Option<usize>,
    multiline: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Kind of the previous edit while consecutive edits may be merged into
    /// one undo step; cleared by caret movement.
    group: Option<EditKind>,
}

impl TextEditor {
    /// An editor holding `text` with the caret at its end.
    pub fn new(text: impl Into<String>) -> Self {
        let text = sanitize(&text.into(), false);
        let end = text.len();
        TextEditor {
            text,
            cursor: end,
            anchor: end,
            ..Default::default()
        }
    }

    /// Limits the text to `max` characters (Unicode scalar values).
    pub fn set_max_chars(&mut self, max: Option<usize>) {
        self.max_chars = max;
    }

    /// Allows line breaks (`\n`; `\r\n` and `\r` are normalized) and tabs.
    /// Switching to single-line removes them from the current text.
    pub fn set_multiline(&mut self, multiline: bool) {
        self.multiline = multiline;
        let text = sanitize(&self.text, multiline);
        if text != self.text {
            self.text = text;
            self.cursor = self.snap(self.cursor.min(self.text.len()));
            self.anchor = self.snap(self.anchor.min(self.text.len()));
        }
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Byte offset of the caret.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Byte offset of the fixed end of the selection.
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// The selected byte range (empty when nothing is selected).
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    /// Replaces the whole text from outside (e.g. a bound signal changed).
    /// Clears history; keeps the caret where possible.
    pub fn set_text(&mut self, text: &str) {
        let text = sanitize(text, self.multiline);
        if text == self.text {
            return;
        }
        self.text = text;
        self.cursor = self.snap(self.cursor.min(self.text.len()));
        self.anchor = self.snap(self.anchor.min(self.text.len()));
        self.undo.clear();
        self.redo.clear();
        self.group = None;
    }

    // ---- caret movement ---------------------------------------------------------

    /// Moves the caret to `offset`, extending the selection when `extend`.
    pub fn move_to(&mut self, offset: usize, extend: bool) {
        self.cursor = self.snap(offset.min(self.text.len()));
        if !extend {
            self.anchor = self.cursor;
        }
        self.group = None;
    }

    /// Sets the selection explicitly (`anchor` stays, `cursor` moves).
    pub fn select(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.snap(anchor.min(self.text.len()));
        self.cursor = self.snap(cursor.min(self.text.len()));
        self.group = None;
    }

    pub fn select_all(&mut self) {
        self.select(0, self.text.len());
    }

    /// One grapheme (or word) to the left. Without `extend`, a selection
    /// collapses to its start instead.
    pub fn move_left(&mut self, extend: bool, word: bool) {
        let target = if !extend && !word && self.has_selection() {
            self.selection().start
        } else if word {
            self.prev_word(self.cursor)
        } else {
            self.prev_grapheme(self.cursor)
        };
        self.move_to(target, extend);
    }

    /// One grapheme (or word) to the right. Without `extend`, a selection
    /// collapses to its end instead.
    pub fn move_right(&mut self, extend: bool, word: bool) {
        let target = if !extend && !word && self.has_selection() {
            self.selection().end
        } else if word {
            self.next_word(self.cursor)
        } else {
            self.next_grapheme(self.cursor)
        };
        self.move_to(target, extend);
    }

    pub fn move_home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }

    pub fn move_end(&mut self, extend: bool) {
        self.move_to(self.text.len(), extend);
    }

    // ---- editing ----------------------------------------------------------------

    /// Inserts `text` at the caret, replacing the selection. Control
    /// characters are removed (line breaks and tabs too, unless multi-line).
    /// Returns whether the text changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let mut text = sanitize(text, self.multiline);
        if let Some(max) = self.max_chars {
            let kept = self.text.chars().count() - self.selected_text().chars().count();
            let room = max.saturating_sub(kept);
            if text.chars().count() > room {
                // Truncate on a grapheme boundary within the remaining room.
                let mut end = 0;
                let mut used = 0;
                for g in text.graphemes(true) {
                    let n = g.chars().count();
                    if used + n > room {
                        break;
                    }
                    used += n;
                    end += g.len();
                }
                text.truncate(end);
            }
        }
        if text.is_empty() && !self.has_selection() {
            return false;
        }
        // A space ends a typing group so undo works word by word.
        let kind = if text.chars().any(char::is_whitespace) || self.has_selection() {
            EditKind::Other
        } else {
            EditKind::Insert
        };
        self.record(kind);
        let range = self.selection();
        self.text.replace_range(range.clone(), &text);
        self.cursor = range.start + text.len();
        self.anchor = self.cursor;
        true
    }

    /// Deletes the selection, or the grapheme (word) before the caret.
    pub fn backspace(&mut self, word: bool) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let start = if word {
            self.prev_word(self.cursor)
        } else {
            self.prev_grapheme(self.cursor)
        };
        self.delete_range(start..self.cursor)
    }

    /// Deletes the selection, or the grapheme (word) after the caret.
    pub fn delete(&mut self, word: bool) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let end = if word {
            self.next_word(self.cursor)
        } else {
            self.next_grapheme(self.cursor)
        };
        self.delete_range(self.cursor..end)
    }

    pub fn delete_selection(&mut self) -> bool {
        let range = self.selection();
        self.group = None;
        self.delete_range(range)
    }

    fn delete_range(&mut self, range: Range<usize>) -> bool {
        if range.is_empty() {
            return false;
        }
        self.record(EditKind::Delete);
        self.text.replace_range(range.clone(), "");
        self.cursor = range.start;
        self.anchor = range.start;
        true
    }

    // ---- history ----------------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.cursor = s.cursor;
        self.anchor = s.anchor;
        self.group = None;
    }

    fn record(&mut self, kind: EditKind) {
        let merge = kind != EditKind::Other && self.group == Some(kind);
        if !merge {
            self.undo.push(self.snapshot());
            if self.undo.len() > MAX_HISTORY {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.group = (kind != EditKind::Other).then_some(kind);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Reverts the last edit group. Returns whether anything changed.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    /// Re-applies the last undone edit group.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        true
    }

    // ---- boundaries -------------------------------------------------------------

    /// The closest grapheme boundary at or before `offset`.
    pub fn snap(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        let mut last = 0;
        for (i, _) in self.text.grapheme_indices(true) {
            if i > offset {
                break;
            }
            last = i;
        }
        if offset == self.text.len() {
            offset
        } else {
            last
        }
    }

    pub fn prev_grapheme(&self, offset: usize) -> usize {
        self.text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    pub fn next_grapheme(&self, offset: usize) -> usize {
        self.text[offset..]
            .graphemes(true)
            .next()
            .map_or(offset, |g| offset + g.len())
    }

    /// Start of the word before `offset` (skipping spaces and punctuation).
    pub fn prev_word(&self, offset: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .rev()
            .find(|(i, w)| *i < offset && is_word(w))
            .map_or(0, |(i, _)| i)
    }

    /// End of the word after `offset` (skipping spaces and punctuation).
    pub fn next_word(&self, offset: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .find(|(i, w)| i + w.len() > offset && is_word(w))
            .map_or(self.text.len(), |(i, w)| i + w.len())
    }

    /// The word (or run of spaces / punctuation) containing `offset`.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        let mut found = None;
        for (i, w) in self.text.split_word_bound_indices() {
            let range = i..i + w.len();
            if range.contains(&offset) {
                return range;
            }
            if range.end == offset {
                found = Some(range);
            }
        }
        found.unwrap_or(offset..offset)
    }
}

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

/// Single-line fields keep printable text only; multi-line fields also keep
/// normalized line breaks and tabs.
fn sanitize(text: &str, multiline: bool) -> String {
    if !text.chars().any(char::is_control) {
        return text.to_string();
    }
    if multiline {
        text.replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect()
    } else {
        text.chars().filter(|c| !c.is_control()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_backspace_respect_graphemes() {
        let mut e = TextEditor::new("");
        e.insert("cafe\u{301}");
        assert_eq!(e.text(), "cafe\u{301}");
        assert!(e.backspace(false));
        assert_eq!(e.text(), "caf", "combining mark is part of the grapheme");
        e.insert("👍🏽");
        e.move_left(false, false);
        assert_eq!(e.cursor(), 3, "emoji with modifier is one step");
        assert!(e.delete(false));
        assert_eq!(e.text(), "caf");
    }

    #[test]
    fn word_navigation_and_deletion() {
        let mut e = TextEditor::new("hello, brave  world");
        e.move_left(false, true);
        assert_eq!(e.cursor(), 14);
        e.move_left(false, true);
        assert_eq!(e.cursor(), 7);
        e.move_right(true, true);
        assert_eq!(e.selected_text(), "brave");
        e.move_end(false);
        assert!(e.backspace(true));
        assert_eq!(e.text(), "hello, brave  ");
        e.move_home(false);
        assert!(e.delete(true));
        assert_eq!(e.text(), ", brave  ");
        assert_eq!(e.word_at(3), 2..7);
    }

    #[test]
    fn selection_is_replaced_and_collapses() {
        let mut e = TextEditor::new("abcdef");
        e.select(1, 4);
        assert_eq!(e.selected_text(), "bcd");
        e.insert("X");
        assert_eq!(e.text(), "aXef");
        assert_eq!(e.cursor(), 2);
        e.select(3, 1);
        e.move_right(false, false);
        assert_eq!((e.cursor(), e.has_selection()), (3, false));
        e.select_all();
        assert!(e.backspace(false));
        assert_eq!(e.text(), "");
    }

    #[test]
    fn undo_groups_typing_by_word() {
        let mut e = TextEditor::new("");
        for c in ["h", "i", " ", "y", "o", "u"] {
            e.insert(c);
        }
        assert!(e.undo());
        assert_eq!(e.text(), "hi ");
        assert!(e.undo());
        assert_eq!(e.text(), "hi");
        assert!(e.undo());
        assert_eq!(e.text(), "");
        assert!(!e.undo());
        assert!(e.redo());
        assert!(e.redo());
        assert_eq!(e.text(), "hi ");
        e.move_home(false);
        e.insert("!");
        assert!(!e.can_redo(), "a new edit clears redo");
    }

    #[test]
    fn caret_movement_breaks_undo_groups() {
        let mut e = TextEditor::new("");
        e.insert("a");
        e.insert("b");
        e.move_left(false, false);
        e.insert("c");
        assert_eq!(e.text(), "acb");
        e.undo();
        assert_eq!(e.text(), "ab");
    }

    #[test]
    fn sanitizes_and_limits_input() {
        let mut e = TextEditor::new("x");
        e.set_max_chars(Some(4));
        e.insert("line\none");
        assert_eq!(e.text(), "xlin");
        assert!(!e.insert("z"));
        e.select(0, 1);
        e.insert("é🙂");
        assert_eq!(e.text(), "élin", "only what fits is inserted");
    }

    #[test]
    fn multiline_keeps_normalized_breaks() {
        let mut e = TextEditor::new("");
        e.set_multiline(true);
        e.insert("a\r\nb\rc\td\u{7}");
        assert_eq!(e.text(), "a\nb\nc\td");
        e.set_multiline(false);
        assert_eq!(e.text(), "abcd");
    }

    #[test]
    fn external_text_clamps_caret() {
        let mut e = TextEditor::new("hello");
        e.insert("!");
        e.set_text("hé");
        assert_eq!(e.cursor(), 3);
        assert!(!e.can_undo());
        e.select(0, 2);
        assert_eq!(e.cursor(), 1, "offsets snap to grapheme boundaries");
    }
}
