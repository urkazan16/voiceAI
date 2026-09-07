//! Case-insensitive substring scanning that always stays on char boundaries.
//!
//! Searching a `to_lowercase()` copy and slicing the original with the offsets
//! it yields is unsound: lowercasing can change the byte length of a string
//! (`İ` is two bytes, its lowercase form `i̇` is three), after which every
//! offset past the first such character points into the middle of a character
//! and slicing panics. These helpers fold one character at a time and only ever
//! return offsets into the original text.

/// Byte range of the first case-insensitive match at or after `from`.
pub fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<(usize, usize)> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    let target: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if target.is_empty() {
        return None;
    }
    haystack[from..].char_indices().find_map(|(offset, _)| {
        let start = from + offset;
        match_at(haystack, start, &target).map(|end| (start, end))
    })
}

fn match_at(haystack: &str, start: usize, target: &[char]) -> Option<usize> {
    let mut matched = 0usize;
    for (offset, ch) in haystack[start..].char_indices() {
        for folded in ch.to_lowercase() {
            if target.get(matched) != Some(&folded) {
                return None;
            }
            matched += 1;
        }
        if matched == target.len() {
            return Some(start + offset + ch.len_utf8());
        }
    }
    None
}

/// True when `start..end` is not glued to surrounding alphanumeric text.
pub fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before_ok = start == 0
        || text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_alphanumeric());
    let after_ok = end >= text.len()
        || text[end..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_alphanumeric());
    before_ok && after_ok
}

/// Replace every whole-word, case-insensitive occurrence of `needle`.
pub fn replace_word_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(haystack.len());
    let mut idx = 0;
    while let Some((start, end)) = find_ci(haystack, needle, idx) {
        out.push_str(&haystack[idx..start]);
        if is_word_boundary(haystack, start, end) {
            out.push_str(replacement);
        } else {
            out.push_str(&haystack[start..end]);
        }
        idx = end;
    }
    out.push_str(&haystack[idx..]);
    out
}

/// Delete every whole-word, case-insensitive occurrence of `needle`.
pub fn remove_word_ci(haystack: &str, needle: &str) -> String {
    replace_word_ci(haystack, needle, "")
}

/// Count whole-word, case-insensitive occurrences of `needle`.
pub fn count_word_ci(haystack: &str, needle: &str) -> usize {
    let mut idx = 0;
    let mut n = 0;
    while let Some((start, end)) = find_ci(haystack, needle, idx) {
        if is_word_boundary(haystack, start, end) {
            n += 1;
        }
        idx = end;
    }
    n
}

/// Byte offset of the first whole-word, case-insensitive occurrence.
pub fn find_word_ci(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let mut idx = 0;
    while let Some((start, end)) = find_ci(haystack, needle, idx) {
        if is_word_boundary(haystack, start, end) {
            return Some((start, end));
        }
        idx = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_matches_regardless_of_case() {
        assert_eq!(find_ci("Hello World", "world", 0), Some((6, 11)));
        assert_eq!(find_ci("Привет Мир", "мир", 0), Some((13, 19)));
        assert_eq!(find_ci("abc", "zzz", 0), None);
    }

    #[test]
    fn survives_characters_that_grow_when_lowercased() {
        // The bug this module exists for: `İ`.to_lowercase() is longer than `İ`,
        // so offsets from a lowercased copy land mid-character in the original.
        let text = "İIİ точка ком";
        assert_eq!(replace_word_ci(text, "точка", "."), "İIİ . ком");
        assert_eq!(count_word_ci(text, "точка"), 1);
        assert_eq!(
            replace_word_ci("ÅSTRÖM İD точка", "точка", "."),
            "ÅSTRÖM İD ."
        );
    }

    #[test]
    fn respects_word_boundaries() {
        assert_eq!(replace_word_ci("точка зрения", "точка", "."), ". зрения");
        assert_eq!(replace_word_ci("точками", "точка", "."), "точками");
        assert_eq!(count_word_ci("ideal candidate", "idea"), 0);
    }

    #[test]
    fn replaces_every_occurrence() {
        assert_eq!(replace_word_ci("a dot b DOT c", "dot", "."), "a . b . c");
        assert_eq!(remove_word_ci("ну ладно ну", "ну"), " ладно ");
    }

    #[test]
    fn empty_needle_is_a_no_op() {
        assert_eq!(replace_word_ci("abc", "", "x"), "abc");
        assert_eq!(find_ci("abc", "", 0), None);
    }
}
