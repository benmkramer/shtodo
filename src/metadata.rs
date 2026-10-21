//! todo.txt-style tokens derived from canonical task text.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Priority(u8);

impl Priority {
    pub(crate) fn letter(self) -> char {
        char::from(self.0)
    }
}

pub(crate) fn priority(text: &str) -> Option<Priority> {
    match text.as_bytes() {
        [b'(', letter @ b'A'..=b'Z', b')', b' ', ..] => Some(Priority(*letter)),
        _ => None,
    }
}

const TRAILING_PUNCTUATION: [char; 6] = ['.', ',', ';', ':', '!', '?'];

pub(crate) fn tags(text: &str) -> impl Iterator<Item = &str> {
    // Most texts have no sigil; one byte scan is cheaper than splitting on Unicode whitespace.
    let candidate = text.bytes().any(|byte| matches!(byte, b'+' | b'@'));
    let text = if candidate { text } else { "" };
    text.split_whitespace().filter_map(normalized_tag)
}

/// Derived tags in text order, keeping the first of each exact duplicate.
pub(crate) fn distinct_tags(text: &str) -> Vec<&str> {
    let mut distinct = Vec::new();
    for tag in tags(text) {
        if !distinct.contains(&tag) {
            distinct.push(tag);
        }
    }
    distinct
}

fn normalized_tag(token: &str) -> Option<&str> {
    if !token.starts_with(['+', '@']) {
        return None;
    }
    let tag = token.trim_end_matches(TRAILING_PUNCTUATION);
    (tag.len() > 1).then_some(tag)
}

const NO_PRIORITY: u32 = 1 << 26;

/// Bits 0..26 are A..Z; bit 26 is "no priority". Empty means any priority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PrioritySet(u32);

impl PrioritySet {
    fn range(start: u8, end: u8) -> Self {
        Self((start..=end).map(|letter| 1 << (letter - b'A')).sum())
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) fn contains(self, priority: Option<Priority>) -> bool {
        let bit = priority.map_or(NO_PRIORITY, |priority| 1 << (priority.0 - b'A'));
        self.is_empty() || self.0 & bit != 0
    }
}

/// One space-free token of the shared filter grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FilterToken<'a> {
    /// A normalized `+project` or `@context` tag.
    Tag(&'a str),
    Priorities(PrioritySet),
}

pub(crate) fn parse_filter_token(token: &str) -> Option<FilterToken<'_>> {
    if token.contains(char::is_whitespace) {
        return None;
    }
    if let Some(tag) = normalized_tag(token) {
        return Some(FilterToken::Tag(tag));
    }
    if token.eq_ignore_ascii_case("none") {
        return Some(FilterToken::Priorities(PrioritySet(NO_PRIORITY)));
    }
    let (start, end) = match token.as_bytes() {
        [letter] => (*letter, *letter),
        [start, b'-', end] => (*start, *end),
        _ => return None,
    };
    let (start, end) = (start.to_ascii_uppercase(), end.to_ascii_uppercase());
    (start.is_ascii_uppercase() && end.is_ascii_uppercase() && start <= end)
        .then(|| FilterToken::Priorities(PrioritySet::range(start, end)))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TaskFilter {
    pub(crate) tags: Vec<String>,
    pub(crate) priorities: PrioritySet,
}

impl TaskFilter {
    pub(crate) fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.priorities.is_empty()
    }

    pub(crate) fn add(&mut self, token: FilterToken<'_>) {
        match token {
            FilterToken::Tag(tag) => {
                if !self.tags.iter().any(|existing| existing == tag) {
                    self.tags.push(tag.to_owned());
                }
            }
            FilterToken::Priorities(set) => self.priorities.0 |= set.0,
        }
    }

    /// Tags are an AND, priorities an OR; the empty filter does not scan the text.
    pub(crate) fn matches(&self, text: &str) -> bool {
        self.priorities.contains(priority(text)) && self.has_tags(text)
    }

    /// Scans the text's tags once, marking required tags until all are found.
    fn has_tags(&self, text: &str) -> bool {
        let required = self.tags.len();
        if required == 0 {
            return true;
        }
        if required > u64::BITS as usize {
            return self
                .tags
                .iter()
                .all(|required| tags(text).any(|tag| tag == required));
        }
        let all = u64::MAX >> (u64::BITS as usize - required);
        let mut found = 0_u64;
        for tag in tags(text) {
            if let Some(index) = self.tags.iter().position(|required| required == tag) {
                found |= 1 << index;
                if found == all {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(text: &str) -> Option<char> {
        priority(text).map(Priority::letter)
    }

    fn filter(tokens: &[&str]) -> TaskFilter {
        let mut filter = TaskFilter::default();
        for token in tokens {
            filter.add(parse_filter_token(token).unwrap());
        }
        filter
    }

    #[test]
    fn priority_should_require_an_uppercase_marker_and_space_at_the_start() {
        assert_eq!(letters("(A) x"), Some('A'));
        assert_eq!(letters("(Z) x"), Some('Z'));
        for text in ["(A)", "(a) x", "(AA) x", "x (A) y", "(A)x", "(A)\tx", "A x"] {
            assert_eq!(letters(text), None, "{text:?}");
        }
        assert!(priority("(A) x") < priority("(B) x"));
    }

    #[test]
    fn tags_should_need_a_sigil_at_token_start_and_a_character_after_it() {
        let found: Vec<_> = tags("+p @c + @ C++ a@b.c bob@example.com x+y").collect();
        assert_eq!(found, ["+p", "@c"]);
    }

    #[test]
    fn tags_should_strip_only_trailing_sentence_punctuation() {
        assert_eq!(tags("ship it +shtodo.").collect::<Vec<_>>(), ["+shtodo"]);
        for mark in ['.', ',', ';', ':', '!', '?'] {
            let text = format!("x +tag{mark}");
            assert_eq!(tags(&text).collect::<Vec<_>>(), ["+tag"], "{text:?}");
        }
        assert_eq!(tags("+x?! @y.,").collect::<Vec<_>>(), ["+x", "@y"]);
        assert_eq!(
            tags("+. @!? +a.b +x) +東京!").collect::<Vec<_>>(),
            ["+a.b", "+x)", "+東京"]
        );
    }

    #[test]
    fn tags_should_split_on_unicode_whitespace_and_keep_text_order() {
        let text = "a\t+one\u{3000}@two\u{a0}x +three";
        assert_eq!(tags(text).collect::<Vec<_>>(), ["+one", "@two", "+three"]);
        assert_eq!(distinct_tags("+x +x. @y +x @y!"), ["+x", "@y"]);
        assert!(distinct_tags("no tokens here").is_empty());
    }

    #[test]
    fn filter_tokens_should_parse_tags_letters_ranges_and_none() {
        assert_eq!(
            parse_filter_token("+shtodo"),
            Some(FilterToken::Tag("+shtodo"))
        );
        assert_eq!(
            parse_filter_token("+shtodo."),
            Some(FilterToken::Tag("+shtodo"))
        );
        assert_eq!(
            parse_filter_token("@東京!"),
            Some(FilterToken::Tag("@東京"))
        );
        for token in [
            "work", "+", "@", "+.", "@?!", "C-A", "AA", "A-", "-A", "A-CC", "", "+a b", "A B",
            "nonee", "1", "(A)",
        ] {
            assert_eq!(parse_filter_token(token), None, "{token:?}");
        }
        let set = |token| match parse_filter_token(token) {
            Some(FilterToken::Priorities(set)) => set,
            other => panic!("{token:?} parsed as {other:?}"),
        };
        assert_eq!(set("a"), set("A"));
        assert_eq!(set("b-d"), set("B-D"));
        assert_eq!(set("NONE"), set("none"));
        assert_eq!(set("C-C"), set("C"));
        assert!(set("A-C").contains(priority("(B) x")));
        assert!(!set("A-C").contains(priority("(D) x")));
        assert!(!set("A-C").contains(None));
        assert!(set("none").contains(None));
        assert!(!set("none").contains(priority("(A) x")));
    }

    #[test]
    fn empty_priority_set_should_match_any_priority() {
        let set = PrioritySet::default();
        assert!(set.is_empty());
        assert!(set.contains(None));
        assert!(set.contains(priority("(Q) x")));
    }

    #[test]
    fn task_filter_should_and_tags_or_priorities_and_and_the_categories() {
        assert!(TaskFilter::default().is_empty());
        assert!(TaskFilter::default().matches("anything"));

        let tags = filter(&["+a", "@b"]);
        assert!(!tags.is_empty());
        assert!(tags.matches("x +a @b"));
        assert!(tags.matches("x @b. +a,"));
        assert!(!tags.matches("x +a"));
        assert!(!tags.matches("x +ab @b"));
        assert!(!tags.matches("x +A @b"));

        let priorities = filter(&["A", "c-d", "none"]);
        assert!(priorities.matches("(A) x"));
        assert!(priorities.matches("(D) x"));
        assert!(priorities.matches("plain"));
        assert!(!priorities.matches("(B) x"));

        let three = filter(&["+a", "@b", "@c"]);
        assert!(three.matches("x @b +a y +a @b @c"));
        assert!(three.matches("(A) @c, +a @b."));
        assert!(!three.matches("x @b +a y +a @b @cc"));
        assert!(!three.matches("x +a @b"));

        let many_tokens: Vec<String> = (0..65).map(|index| format!("+t{index}")).collect();
        let many = filter(&many_tokens.iter().map(String::as_str).collect::<Vec<_>>());
        let all_tags = many_tokens.join(" ");
        assert!(many.matches(&all_tags));
        assert!(!many.matches(&many_tokens[..64].join(" ")));
        let first_64 = filter(
            &many_tokens[..64]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        assert!(first_64.matches(&all_tags));
        assert!(!first_64.matches(&many_tokens[1..].join(" ")));

        let both = filter(&["+a", "A-B", "+a."]);
        assert_eq!(both.tags, ["+a"]);
        assert!(both.matches("(B) x +a"));
        assert!(!both.matches("(C) x +a"));
        assert!(!both.matches("(A) x"));
    }
}
