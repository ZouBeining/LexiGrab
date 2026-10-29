//! Best-effort UIA sentence capture for the desktop popup.
//!
//! After the focused text is copied to the clipboard, the runtime may ask this
//! module for the sentence that contains the selection. The work is done on a
//! detached thread so a hung accessibility provider can never block the popup:
//! the caller only waits for [`capture_sentence_budget`] and gets `None` when
//! the budget expires.
//!
//! The captured text is only returned to the caller; it is never logged.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, TextUnit_Paragraph, UIA_TextPatternId,
};

/// Maximum number of characters kept from the enclosing sentence.
const MAX_SENTENCE_CHARS: usize = 300;

/// Read the sentence around the current UIA text selection within `budget`.
///
/// Returns `None` for any failure: no focused text control, no UIA text
/// pattern, no selection, an empty/undecodable paragraph or a timeout. It never
/// panics and never exceeds the budget (a stuck worker thread is detached).
pub fn capture_sentence_budget(budget: Duration) -> Option<String> {
    if budget.is_zero() {
        return None;
    }

    let (sender, receiver) = mpsc::channel();
    let spawn = thread::Builder::new()
        .name("lexigrab-uia-sentence".to_string())
        .spawn(move || {
            let _ = sender.send(read_focused_sentence());
        });
    if spawn.is_err() {
        return None;
    }

    // On timeout the worker thread is left detached; it holds no locks the
    // caller needs and exits on its own once the UIA call returns.
    receiver.recv_timeout(budget).ok().flatten()
}

/// Initialize COM on the worker thread and read the sentence.
fn read_focused_sentence() -> Option<String> {
    // SAFETY: COM is initialized and uninitialized on the same thread.
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
    let sentence = unsafe { read_focused_sentence_com() };
    if initialized {
        // SAFETY: paired with the successful `CoInitializeEx` above.
        unsafe { CoUninitialize() };
    }
    sentence
}

/// COM-dependent part of the capture; every failure simply yields `None`.
unsafe fn read_focused_sentence_com() -> Option<String> {
    // SAFETY: COM was initialized on this thread by the caller.
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()?;
    // SAFETY: the automation object is a valid COM interface for this thread.
    let element = unsafe { automation.GetFocusedElement() }.ok()?;
    // SAFETY: `element` is a valid UIA element; the pattern result is checked.
    let pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
            .ok()?;
    // SAFETY: valid text pattern interface.
    let selection = unsafe { pattern.GetSelection() }.ok()?;
    // SAFETY: valid range array interface.
    if unsafe { selection.Length() }.unwrap_or(0) <= 0 {
        return None;
    }
    // SAFETY: the array length was checked above.
    let range = unsafe { selection.GetElement(0) }.ok()?;
    // SAFETY: the range belongs to the focused text pattern.
    let selected = unsafe { range.GetText(-1) }.ok()?.to_string();
    // SAFETY: same text range instance.
    unsafe { range.ExpandToEnclosingUnit(TextUnit_Paragraph) }.ok()?;
    // SAFETY: same text range instance.
    let paragraph = unsafe { range.GetText(-1) }.ok()?.to_string();

    extract_sentence(&paragraph, &selected)
}

/// Pick the sentence containing `selected` from the expanded paragraph.
///
/// Falls back to the only sentence when the selection cannot be located, and
/// returns `None` when the paragraph is empty or ambiguous. The result is
/// whitespace-collapsed and truncated to [`MAX_SENTENCE_CHARS`] characters.
fn extract_sentence(paragraph: &str, selected: &str) -> Option<String> {
    let collapsed = collapse_whitespace(paragraph);
    if collapsed.is_empty() {
        return None;
    }

    let sentences = split_sentences(&collapsed);
    if sentences.is_empty() {
        return None;
    }

    let selected = collapse_whitespace(selected);
    let chosen = if selected.is_empty() {
        sentences.first().copied()
    } else {
        sentences
            .iter()
            .find(|sentence| sentence.contains(selected.as_str()))
            .copied()
            .or_else(|| {
                // A single sentence is unambiguous even when the selection
                // spelling differs slightly (normalization, ellipsis, ...).
                if sentences.len() == 1 {
                    sentences.first().copied()
                } else {
                    None
                }
            })
    }?;

    let sentence = chosen.trim();
    if sentence.is_empty() {
        return None;
    }
    Some(truncate_chars(sentence, MAX_SENTENCE_CHARS))
}

/// Collapse all whitespace runs into single spaces and trim the result.
fn collapse_whitespace(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = !collapsed.is_empty();
            continue;
        }
        if pending_space {
            collapsed.push(' ');
            pending_space = false;
        }
        collapsed.push(character);
    }
    collapsed
}

/// Split text into sentences at `. ! ?` and CJK `。！？` boundaries.
///
/// Delimiters stay attached to the sentence they end; trailing separators that
/// produce an empty tail are dropped.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if !matches!(character, '.' | '!' | '?' | '。' | '！' | '？') {
            continue;
        }
        let end = index + character.len_utf8();
        let sentence = text[start..end].trim();
        if !sentence.is_empty() {
            sentences.push(sentence);
        }
        start = end;
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
}

/// Truncate to at most `limit` characters without splitting a char boundary.
fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn extracts_sentence_containing_selection() {
        let paragraph =
            "The quick brown fox jumps. A serendipity moment happened here! Then it left.";
        let sentence = extract_sentence(paragraph, "serendipity").expect("sentence");
        assert_eq!(sentence, "A serendipity moment happened here!");
    }

    #[test]
    fn extracts_cjk_sentence() {
        let paragraph = "这是第一句。这是包含目标的第二句！最后一句。";
        let sentence = extract_sentence(paragraph, "目标").expect("sentence");
        assert_eq!(sentence, "这是包含目标的第二句！");
    }

    #[test]
    fn falls_back_to_single_sentence() {
        let sentence = extract_sentence("Only one sentence here", "missing").expect("sentence");
        assert_eq!(sentence, "Only one sentence here");
    }

    #[test]
    fn returns_none_for_ambiguous_paragraph_without_match() {
        assert!(extract_sentence("First sentence. Second sentence.", "absent").is_none());
    }

    #[test]
    fn returns_none_for_empty_paragraph() {
        assert!(extract_sentence("   \n\t ", "anything").is_none());
        assert!(extract_sentence("", "").is_none());
    }

    #[test]
    fn collapses_whitespace_and_truncates() {
        let paragraph = format!("word\n\n   spaced \t sentence {}", "x".repeat(400));
        let sentence = extract_sentence(&paragraph, "spaced").expect("sentence");
        assert!(sentence.starts_with("word spaced sentence"));
        assert_eq!(sentence.chars().count(), MAX_SENTENCE_CHARS);
    }

    #[test]
    fn budget_is_respected_on_zero_budget() {
        let start = Instant::now();
        assert_eq!(capture_sentence_budget(Duration::ZERO), None);
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn budget_is_respected_when_capture_is_attempted() {
        let budget = Duration::from_millis(120);
        let start = Instant::now();
        // Outcome depends on the focused application; only bound the latency.
        let _ = capture_sentence_budget(budget);
        assert!(
            start.elapsed() < Duration::from_millis(1000),
            "capture exceeded budget: {:?}",
            start.elapsed()
        );
    }
}
