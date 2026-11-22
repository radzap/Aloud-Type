//! Segment filtering: drops likely non-speech and known recognizer hallucinations.
use crate::seams::Segment;

/// Phrases Whisper-style models emit over silence or noise (compared after
/// lowercasing and stripping punctuation, against the whole segment).
const HALLUCINATIONS: &[&str] = &[
    "thank you",
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
    "subtitles by the amaraorg community",
    "blank audio",
    "music",
    "you",
    "bye",
];

fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| if c == '_' { ' ' } else { c })
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// True if the segment should be dropped.
pub fn should_drop(segment: &Segment, no_speech_threshold: f32) -> bool {
    segment.no_speech_prob >= no_speech_threshold
        || HALLUCINATIONS.contains(&normalize(&segment.text).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str, p: f32) -> Segment {
        Segment {
            text: text.into(),
            words: vec![],
            no_speech_prob: p,
        }
    }

    #[test]
    fn drops_blocklisted_text_ignoring_case_and_punctuation() {
        assert!(should_drop(&s(" [BLANK_AUDIO] ", 0.0), 0.6));
        assert!(should_drop(&s("Thank you.", 0.0), 0.6));
    }

    #[test]
    fn keeps_real_sentences_that_merely_contain_blocklisted_words() {
        assert!(!should_drop(&s("thank you for the update", 0.0), 0.6));
    }

    #[test]
    fn drops_at_or_above_the_no_speech_threshold() {
        assert!(should_drop(&s("hello", 0.6), 0.6));
        assert!(!should_drop(&s("hello", 0.59), 0.6));
    }
}
