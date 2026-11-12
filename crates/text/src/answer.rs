//! The visible answer of a reply as it streams: thinking is split off, and text that may begin a tag or a stop string is held back.

use crate::{stop_scan, GenerationResult};

/// The tags a thinking model writes around its reasoning inside the text.
const THINK_TAGS: (&str, &str) = ("<think>", "</think>");

/// What a piece settles, in the order of the reply.
#[derive(Debug, PartialEq)]
pub(crate) enum Part {
    Answer(String),
    /// Characters that were thinking: a block's text, or one of its tags.
    Thinking(usize),
    /// A closing tag came without its opening one, which the chat template wrote into the prompt: all answer text before it was thinking.
    EarlierWasThinking,
}

impl Part {
    fn thinking(text: &str) -> Option<Self> {
        (!text.is_empty()).then(|| Self::Thinking(text.chars().count()))
    }
}

/// Splits a reply's pieces into answer and thinking; a `<think>` block and its tags are thinking.
#[derive(Debug, Default)]
pub(crate) struct ThinkingSplitter {
    /// The end of the reply so far that may still begin a tag.
    pending: String,
    inside: bool,
    /// A block has just closed: the white space a model writes after it is not the answer's.
    after_close: bool,
}

impl ThinkingSplitter {
    /// What `piece` settles.
    pub(crate) fn push(&mut self, piece: &str) -> Vec<Part> {
        self.pending.push_str(piece);
        let mut parts = Vec::new();
        loop {
            let tags: &[&str] = if self.inside {
                &[THINK_TAGS.1]
            } else {
                &[THINK_TAGS.0, THINK_TAGS.1]
            };
            let (found, safe) = stop_scan(&self.pending, tags);
            let settled: String = self.pending.drain(..safe).collect();
            parts.extend(self.part(&settled));
            if found.is_none() {
                return parts;
            }
            let closing = self.pending.starts_with(THINK_TAGS.1);
            let tag = if closing { THINK_TAGS.1 } else { THINK_TAGS.0 };
            self.pending.drain(..tag.len());
            if closing && !self.inside {
                parts.push(Part::EarlierWasThinking);
            }
            parts.extend(Part::thinking(tag));
            self.inside = !closing;
            self.after_close = closing;
        }
    }

    /// `text` as thinking inside a block, else as answer without the white space that directly follows a closing tag.
    fn part(&mut self, text: &str) -> Option<Part> {
        if self.inside {
            return Part::thinking(text);
        }
        let text = if self.after_close {
            text.trim_start()
        } else {
            text
        };
        if text.is_empty() {
            return None;
        }
        self.after_close = false;
        Some(Part::Answer(text.to_string()))
    }

    /// What was held back when the reply ended: answer text, or thinking when the block never closed.
    pub(crate) fn finish(&mut self) -> Option<Part> {
        let rest = std::mem::take(&mut self.pending);
        self.part(&rest)
    }
}

/// One reply's answer for both backends: `on_text` receives only text that is neither thinking nor the start of a stop string.
pub(crate) struct Answer<'a> {
    stops: &'a [String],
    thinking: ThinkingSplitter,
    thinking_chars: usize,
    text: String,
    shown: usize,
    stopped: bool,
}

impl<'a> Answer<'a> {
    /// `stops` is empty for an endpoint, whose server ends the reply at a stop string itself.
    pub(crate) fn new(stops: &'a [String]) -> Self {
        Self {
            stops,
            thinking: ThinkingSplitter::default(),
            thinking_chars: 0,
            text: String::new(),
            shown: 0,
            stopped: false,
        }
    }

    /// Takes the next piece of the reply; true once a stop string has ended the answer, and nothing after it counts.
    pub(crate) fn push(&mut self, piece: &str, on_text: &mut dyn FnMut(&str)) -> bool {
        if self.stopped {
            return true;
        }
        for part in self.thinking.push(piece) {
            if self.take(part, on_text) {
                break;
            }
        }
        self.stopped
    }

    /// Takes thinking a server sent apart from the text.
    pub(crate) fn push_thinking(&mut self, thinking: &str) {
        self.thinking_chars += thinking.chars().count();
    }

    fn take(&mut self, part: Part, on_text: &mut dyn FnMut(&str)) -> bool {
        match part {
            Part::Answer(answer) => return self.show(&answer, on_text),
            Part::Thinking(chars) => self.thinking_chars += chars,
            // What `on_text` received stays shown: nothing could tell it was thinking before the tag came.
            Part::EarlierWasThinking => {
                self.thinking_chars += self.text.chars().count();
                self.text.clear();
                self.shown = 0;
            }
        }
        self.stopped
    }

    fn show(&mut self, answer: &str, on_text: &mut dyn FnMut(&str)) -> bool {
        self.text.push_str(answer);
        let (stop_at, safe) = stop_scan(&self.text, self.stops);
        if safe > self.shown {
            on_text(&self.text[self.shown..safe]);
            self.shown = safe;
        }
        if let Some(at) = stop_at {
            self.text.truncate(at);
            self.stopped = true;
        }
        self.stopped
    }

    /// Ends the reply: shows what was held back and writes the answer and its thinking count into `result`; true when a stop string ended it.
    pub(crate) fn finish(
        mut self,
        on_text: &mut dyn FnMut(&str),
        result: &mut GenerationResult,
    ) -> bool {
        if !self.stopped {
            if let Some(part) = self.thinking.finish() {
                self.take(part, on_text);
            }
        }
        if !self.stopped && self.shown < self.text.len() {
            on_text(&self.text[self.shown..]);
        }
        result.text = self.text;
        result.thinking_chars = self.thinking_chars;
        self.stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FinishReason;

    fn empty_result() -> GenerationResult {
        GenerationResult {
            text: String::new(),
            tokens: Vec::new(),
            finish_reason: FinishReason::Stop,
            prompt_tokens: 0,
            completion_tokens: 0,
            time_to_first_token_ms: 0.0,
            decode_tokens_per_second: 0.0,
            thinking_chars: 0,
        }
    }

    /// Runs `pieces` through an answer: what `on_text` received, the answer, the thinking count, and whether a stop string ended it.
    fn replay(pieces: &[&str], stops: &[&str]) -> (Vec<String>, String, usize, bool) {
        let stops: Vec<String> = stops.iter().map(ToString::to_string).collect();
        let mut result = empty_result();
        let mut shown = Vec::new();
        let mut on_text = |text: &str| shown.push(text.to_string());
        let mut answer = Answer::new(&stops);
        for piece in pieces {
            if answer.push(piece, &mut on_text) {
                break;
            }
        }
        let stopped = answer.finish(&mut on_text, &mut result);
        (shown, result.text, result.thinking_chars, stopped)
    }

    #[test]
    fn a_think_block_is_counted_and_left_out_of_the_answer() {
        let (shown, text, thinking, _) =
            replay(&["<think>", "hm, cats", "</think>", "Twice."], &[]);
        assert_eq!(shown, ["Twice."]);
        assert_eq!(text, "Twice.");
        assert_eq!(thinking, "<think>hm, cats</think>".chars().count());
        // One piece may hold the whole block, text before it and text after it.
        let (shown, text, thinking, _) = replay(&["A <think>x</think>B"], &[]);
        assert_eq!((shown.concat().as_str(), text.as_str()), ("A B", "A B"));
        assert_eq!(thinking, "<think>x</think>".len());
    }

    #[test]
    fn a_tag_split_over_two_pieces_is_held_back_until_it_is_decided() {
        let (shown, text, thinking, _) = replay(&["Hi <th", "ink>secret</thi", "nk> there"], &[]);
        assert_eq!(shown, ["Hi ", "there"]);
        assert_eq!(text, "Hi there");
        assert_eq!(thinking, "<think>secret</think>".len());
    }

    #[test]
    fn a_closing_tag_alone_makes_everything_before_it_thinking() {
        // Qwen3's template opens the block in the prompt on some servers, so the reply holds only the end of it.
        let (shown, text, thinking, _) = replay(
            &["Okay, the user", " asks.\n", "</think>", "\n\n", "Twice."],
            &[],
        );
        assert_eq!(shown, ["Okay, the user", " asks.\n", "Twice."]);
        assert_eq!(text, "Twice.");
        assert_eq!(thinking, "Okay, the user asks.\n</think>".len());
        // In one piece the earlier text is still shown first: a stop string in it must end the reply as it would piece by piece.
        let (shown, text, thinking, stopped) = replay(&["hm</think>\n\nTwice."], &["END"]);
        assert_eq!(shown, ["hm", "Twice."]);
        assert_eq!((text.as_str(), thinking, stopped), ("Twice.", 10, false));
        let (shown, text, thinking, stopped) = replay(&["one END two</think>three"], &["END"]);
        assert_eq!((shown.concat().as_str(), text.as_str()), ("one ", "one "));
        assert_eq!((thinking, stopped), (0, true));
    }

    #[test]
    fn white_space_after_a_closing_tag_is_dropped_also_piece_by_piece() {
        let (shown, text, thinking, _) = replay(
            &["<think>x</think>", "\n", "\n ", "Twice.\n", " Or so."],
            &[],
        );
        assert_eq!(shown, ["Twice.\n", " Or so."]);
        assert_eq!(text, "Twice.\n Or so.");
        // The dropped white space is neither answer nor thinking.
        assert_eq!(thinking, "<think>x</think>".len());
        let (shown, text, _, _) = replay(&["<think>x</think>\n\n"], &[]);
        assert!(shown.is_empty() && text.is_empty(), "{shown:?} {text:?}");
        // White space anywhere else is the model's own.
        let (_, text, _, _) = replay(&["\n A\n\nB "], &[]);
        assert_eq!(text, "\n A\n\nB ");
    }

    #[test]
    fn text_that_only_begins_like_a_tag_is_answer_once_it_is_decided() {
        let (shown, text, thinking, _) = replay(&["a <", "b and <thin", " air", " <"], &[]);
        assert_eq!(shown, ["a ", "<b and ", "<thin air", " ", "<"]);
        assert_eq!(text, "a <b and <thin air <");
        assert_eq!(thinking, 0);
    }

    #[test]
    fn an_unclosed_think_block_leaves_an_empty_answer_and_a_thinking_count() {
        let (shown, text, thinking, _) =
            replay(&["<think>", "the log says \u{e9}t\u{e9}", " </thi"], &[]);
        assert!(shown.is_empty(), "{shown:?}");
        assert_eq!(text, "");
        assert_eq!(
            thinking,
            "<think>the log says \u{e9}t\u{e9} </thi".chars().count()
        );
    }

    #[test]
    fn a_stop_string_ends_the_answer_and_is_not_looked_for_in_thinking() {
        let (shown, text, thinking, stopped) =
            replay(&["<think>END</think>", "one E", "ND two"], &["END"]);
        assert_eq!(shown, ["one "]);
        assert_eq!(text, "one ");
        assert_eq!(thinking, "<think>END</think>".len());
        assert!(stopped);
        // A held-back start of a stop string is shown when the reply ends without the rest.
        let (shown, text, _, stopped) = replay(&["one E"], &["END"]);
        assert_eq!((shown.concat().as_str(), text.as_str()), ("one E", "one E"));
        assert!(!stopped);
    }

    #[test]
    fn a_stop_string_that_ends_in_a_held_back_tag_start_still_ends_the_answer() {
        let (shown, text, _, stopped) = replay(&["yes a<"], &["a<"]);
        assert_eq!((shown.concat().as_str(), text.as_str()), ("yes ", "yes "));
        assert!(stopped);
    }

    #[test]
    fn a_reply_cut_into_single_characters_gives_the_answer_of_the_whole_reply() {
        for reply in [
            "",
            "plain words",
            "<think>hm \u{e9}t\u{e9}</think>\n\nTwice.",
            "a < b <thin air <think>x</think> c </think> d<",
            "<think>never closed </thi",
            "<<think>><</think>>",
            "<think>one</think>two<think>three</think>four END five",
            "hm, cats</think>\n\nTwice.",
            "<think>a</think> \n b </think>\n c END d",
            "one END two</think>three",
        ] {
            let whole = replay(&[reply], &["END"]);
            let cut: Vec<String> = reply.chars().map(String::from).collect();
            let cut: Vec<&str> = cut.iter().map(String::as_str).collect();
            let (shown, text, thinking, stopped) = replay(&cut, &["END"]);
            // Only a closing tag without its opening one leaves more shown than the answer holds.
            assert!(shown.concat().ends_with(&text), "{reply}");
            assert!(whole.0.concat().ends_with(&whole.1), "{reply}");
            assert_eq!(
                (text, thinking, stopped),
                (whole.1, whole.2, whole.3),
                "{reply}"
            );
        }
    }

    #[test]
    fn thinking_a_server_sends_apart_is_counted_and_never_shown() {
        let stops: Vec<String> = Vec::new();
        let mut result = empty_result();
        let mut shown = String::new();
        let mut answer = Answer::new(&stops);
        answer.push_thinking("hm, \u{e9}t\u{e9}");
        answer.push("Twice.", &mut |text| shown.push_str(text));
        answer.finish(&mut |text| shown.push_str(text), &mut result);
        assert_eq!((shown.as_str(), result.text.as_str()), ("Twice.", "Twice."));
        assert_eq!(result.thinking_chars, 7);
    }
}
