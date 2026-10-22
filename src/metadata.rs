//! todo.txt-style tokens derived from canonical task text.

use std::{borrow::Cow, fmt};

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

/// A proleptic Gregorian calendar date; field order makes the derived `Ord` chronological.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    pub(crate) const MIN: Self = Self {
        year: 0,
        month: 1,
        day: 1,
    };
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "shell list filters use it in Stage 2a")
    )]
    pub(crate) const MAX: Self = Self {
        year: 9999,
        month: 12,
        day: 31,
    };

    pub(crate) fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        (year <= 9999
            && (1..=12).contains(&month)
            && (1..=days_in_month(year, month)).contains(&day))
        .then_some(Self { year, month, day })
    }

    /// Exactly `YYYY-MM-DD` in ASCII digits, and a real calendar day.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let &[y0, y1, y2, y3, b'-', m0, m1, b'-', d0, d1] = text.as_bytes() else {
            return None;
        };
        let digit = |byte: u8| byte.is_ascii_digit().then(|| u16::from(byte - b'0'));
        let year = digit(y0)? * 1000 + digit(y1)? * 100 + digit(y2)? * 10 + digit(y3)?;
        let month = digit(m0)? * 10 + digit(m1)?;
        let day = digit(d0)? * 10 + digit(d1)?;
        Self::new(year, u8::try_from(month).ok()?, u8::try_from(day).ok()?)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "shell list filters use it in Stage 2a")
    )]
    pub(crate) fn succ(self) -> Option<Self> {
        Self::new(self.year, self.month, self.day + 1)
            .or_else(|| Self::new(self.year, self.month + 1, 1))
            .or_else(|| Self::new(self.year.checked_add(1)?, 1, 1))
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "shell list filters use it in Stage 2a")
    )]
    pub(crate) fn pred(self) -> Option<Self> {
        if self.day > 1 {
            return Some(Self {
                day: self.day - 1,
                ..self
            });
        }
        let (year, month) = match self.month {
            1 => (self.year.checked_sub(1)?, 12),
            month => (self.year, month - 1),
        };
        Self::new(year, month, days_in_month(year, month))
    }
}

impl fmt::Display for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

const DUE_KEY: &str = "due:";

/// The first valid due token as its `due:YYYY-MM-DD` slice (for row styling) and its date.
pub(crate) fn due_token(text: &str) -> Option<(&str, Date)> {
    // Most texts have no key; `contains` is the cheapest rejection, and searching for the key
    // avoids splitting the whole text on Unicode whitespace.
    if !text.contains(DUE_KEY) {
        return None;
    }
    text.match_indices(DUE_KEY).find_map(|(start, _)| {
        let token_start = text[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        if !token_start {
            return None;
        }
        let token = text[start..].split(char::is_whitespace).next()?;
        let value = token[DUE_KEY.len()..].trim_end_matches(TRAILING_PUNCTUATION);
        let date = Date::parse(value)?;
        Some((&token[..DUE_KEY.len() + value.len()], date))
    })
}

pub(crate) fn due(text: &str) -> Option<Date> {
    due_token(text).map(|(_, date)| date)
}

/// Rewrites `due:today` and `due:tomorrow` (ASCII case-insensitive words) to dates at add and
/// edit time. `today` is called at most once, and only when such a word is present.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "shell and TUI add and edit use it in Stage 2b")
)]
pub(crate) fn normalize_due(text: &str, mut today: impl FnMut() -> Date) -> Cow<'_, str> {
    if !text.contains(DUE_KEY) {
        return Cow::Borrowed(text);
    }
    let mut resolved = None;
    let mut normalized = String::new();
    let mut copied = 0;
    for token in text.split_whitespace() {
        let Some(value) = token.strip_prefix(DUE_KEY) else {
            continue;
        };
        let word = value.trim_end_matches(TRAILING_PUNCTUATION);
        let date = if word.eq_ignore_ascii_case("today") {
            Some(*resolved.get_or_insert_with(&mut today))
        } else if word.eq_ignore_ascii_case("tomorrow") {
            resolved.get_or_insert_with(&mut today).succ()
        } else {
            None
        };
        let Some(date) = date else {
            continue;
        };
        // Tokens borrow `text`, so the word's offset comes from the slice addresses.
        let start = word.as_ptr().addr() - text.as_ptr().addr();
        normalized.push_str(&text[copied..start]);
        normalized.push_str(&date.to_string());
        copied = start + word.len();
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    normalized.push_str(&text[copied..]);
    Cow::Owned(normalized)
}

/// One `--due` value, or the value of a `due:` filter-editor token, before `today` is known.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "shell list filters use it in Stage 2a")
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DueTerm {
    /// No valid due token.
    None,
    /// Any valid due token.
    Any,
    /// Open, and due before today.
    Overdue,
    /// Due within the inclusive range; an omitted bound is unbounded.
    Range {
        start: Option<DueBound>,
        end: Option<DueBound>,
    },
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "shell list filters use it in Stage 2a")
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DueBound {
    Today,
    Tomorrow,
    Date(Date),
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "shell list filters use it in Stage 2a")
)]
impl DueBound {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("today") {
            Some(Self::Today)
        } else if value.eq_ignore_ascii_case("tomorrow") {
            Some(Self::Tomorrow)
        } else {
            Date::parse(value).map(Self::Date)
        }
    }

    fn resolve(self, today: Date) -> Option<Date> {
        match self {
            Self::Today => Some(today),
            Self::Tomorrow => today.succ(),
            Self::Date(date) => Some(date),
        }
    }

    /// Ordered without knowing today, when both bounds are absolute or both relative.
    fn known_after(self, other: Self) -> bool {
        match (self, other) {
            (Self::Date(start), Self::Date(end)) => start > end,
            (Self::Tomorrow, Self::Today) => true,
            _ => false,
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "shell list filters use it in Stage 2a")
)]
pub(crate) fn parse_due_term(value: &str) -> Option<DueTerm> {
    for (word, term) in [
        ("none", DueTerm::None),
        ("any", DueTerm::Any),
        ("overdue", DueTerm::Overdue),
    ] {
        if value.eq_ignore_ascii_case(word) {
            return Some(term);
        }
    }
    let Some((start, end)) = value.split_once("..") else {
        let bound = DueBound::parse(value)?;
        return Some(DueTerm::Range {
            start: Some(bound),
            end: Some(bound),
        });
    };
    let bound = |value: &str| match value {
        "" => Some(None),
        value => DueBound::parse(value).map(Some),
    };
    let (start, end) = (bound(start)?, bound(end)?);
    match (start, end) {
        (None, None) => None,
        (Some(start), Some(end)) if start.known_after(end) => None,
        _ => Some(DueTerm::Range { start, end }),
    }
}

/// A resolved inclusive span; `start > end` is empty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DueSpan {
    start: Date,
    end: Date,
    open_only: bool,
}

/// `--due` terms, a union; empty means any. Terms stay unresolved so a new day only re-resolves.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DueFilter {
    terms: Vec<DueTerm>,
    /// Spans for `terms` on one local date. Unresolved terms match nothing (fail closed).
    spans: Option<Vec<DueSpan>>,
}

impl DueFilter {
    pub(crate) fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "shell list filters use it in Stage 2a")
    )]
    pub(crate) fn add(&mut self, term: DueTerm) {
        if !self.terms.contains(&term) {
            self.terms.push(term);
        }
        self.spans = None;
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "shell list filters use it in Stage 2a")
    )]
    /// Calls `today` only when a term depends on it, so absolute filters never read the clock.
    pub(crate) fn resolve(&mut self, today: impl FnOnce() -> Date) {
        let relative = |bound: Option<DueBound>| !matches!(bound, None | Some(DueBound::Date(_)));
        let today = self
            .terms
            .iter()
            .any(|term| match *term {
                DueTerm::Overdue => true,
                DueTerm::Range { start, end } => relative(start) || relative(end),
                DueTerm::None | DueTerm::Any => false,
            })
            .then(today);
        let at = |bound: Option<DueBound>, unbounded| match (bound, today) {
            (None, _) => Some(unbounded),
            (Some(DueBound::Date(date)), _) => Some(date),
            (Some(bound), Some(today)) => bound.resolve(today),
            (Some(_), None) => None,
        };
        // A bound that does not exist (the day before 0000-01-01) makes the span empty.
        let span = |start: Option<Date>, end: Option<Date>, open_only| match (start, end) {
            (Some(start), Some(end)) => DueSpan {
                start,
                end,
                open_only,
            },
            _ => DueSpan {
                start: Date::MAX,
                end: Date::MIN,
                open_only,
            },
        };
        let spans = self.terms.iter().filter_map(|term| match *term {
            DueTerm::None => None,
            DueTerm::Any => Some(span(Some(Date::MIN), Some(Date::MAX), false)),
            DueTerm::Overdue => Some(span(Some(Date::MIN), today.and_then(Date::pred), true)),
            DueTerm::Range { start, end } => {
                Some(span(at(start, Date::MIN), at(end, Date::MAX), false))
            }
        });
        self.spans = Some(spans.collect());
    }

    pub(crate) fn matches(&self, due: Option<Date>, completed: bool) -> bool {
        if self.is_empty() {
            return true;
        }
        let Some(spans) = &self.spans else {
            return false;
        };
        match due {
            None => self.terms.contains(&DueTerm::None),
            Some(date) => spans.iter().any(|span| {
                !(span.open_only && completed) && span.start <= date && date <= span.end
            }),
        }
    }
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
    pub(crate) due: DueFilter,
}

impl TaskFilter {
    pub(crate) fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.priorities.is_empty() && self.due.is_empty()
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

    /// Tags are an AND; priorities and due terms each an OR. Checks run cheapest first, and an
    /// empty category does not scan the text. `completed` matters only to `overdue`.
    pub(crate) fn matches(&self, text: &str, completed: bool) -> bool {
        debug_assert!(
            self.due.is_empty() || self.due.spans.is_some(),
            "due filter used before resolve"
        );
        self.priorities.contains(priority(text))
            && (self.due.is_empty() || self.due.matches(due(text), completed))
            && self.has_tags(text)
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
        assert!(TaskFilter::default().matches("anything", false));

        let tags = filter(&["+a", "@b"]);
        assert!(!tags.is_empty());
        assert!(tags.matches("x +a @b", false));
        assert!(tags.matches("x @b. +a,", false));
        assert!(!tags.matches("x +a", false));
        assert!(!tags.matches("x +ab @b", false));
        assert!(!tags.matches("x +A @b", false));

        let priorities = filter(&["A", "c-d", "none"]);
        assert!(priorities.matches("(A) x", false));
        assert!(priorities.matches("(D) x", false));
        assert!(priorities.matches("plain", false));
        assert!(!priorities.matches("(B) x", false));

        let three = filter(&["+a", "@b", "@c"]);
        assert!(three.matches("x @b +a y +a @b @c", false));
        assert!(three.matches("(A) @c, +a @b.", false));
        assert!(!three.matches("x @b +a y +a @b @cc", false));
        assert!(!three.matches("x +a @b", false));

        let many_tokens: Vec<String> = (0..65).map(|index| format!("+t{index}")).collect();
        let many = filter(&many_tokens.iter().map(String::as_str).collect::<Vec<_>>());
        let all_tags = many_tokens.join(" ");
        assert!(many.matches(&all_tags, false));
        assert!(!many.matches(&many_tokens[..64].join(" "), false));
        let first_64 = filter(
            &many_tokens[..64]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        assert!(first_64.matches(&all_tags, false));
        assert!(!first_64.matches(&many_tokens[1..].join(" "), false));

        let both = filter(&["+a", "A-B", "+a."]);
        assert_eq!(both.tags, ["+a"]);
        assert!(both.matches("(B) x +a", false));
        assert!(!both.matches("(C) x +a", false));
        assert!(!both.matches("(A) x", false));
    }

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    fn resolved(values: &[&str], today: &str) -> DueFilter {
        let mut filter = DueFilter::default();
        for value in values {
            filter.add(parse_due_term(value).unwrap());
        }
        filter.resolve(|| date(today));
        filter
    }

    #[test]
    fn date_parse_should_accept_only_real_ascii_iso_days() {
        assert_eq!(Date::parse("2026-10-21"), Date::new(2026, 10, 21));
        assert!(Date::parse("2024-02-29").is_some());
        assert!(Date::parse("2000-02-29").is_some());
        assert_eq!(Date::parse("0000-01-01"), Some(Date::MIN));
        assert_eq!(Date::parse("9999-12-31"), Some(Date::MAX));
        for text in [
            "2026-02-29",
            "1900-02-29",
            "2026-00-10",
            "2026-13-01",
            "2026-10-00",
            "2026-04-31",
            "2026-1-05",
            "26-10-21",
            "2026/10/21",
            " 2026-10-21",
            "2026-10-21 ",
            "２０２６-10-21",
            "+2026-10-21",
            "-2026-10-21",
            "",
        ] {
            assert_eq!(Date::parse(text), None, "{text:?}");
        }
        assert_eq!(Date::new(10_000, 1, 1), None);
        assert!(date("2026-10-21") < date("2026-10-22"));
        assert!(date("2026-09-30") < date("2026-10-01"));
        assert!(date("2025-12-31") < date("2026-01-01"));
    }

    #[test]
    fn date_succ_and_pred_should_cross_month_year_and_leap_boundaries() {
        let step = |text, forward: bool| {
            let day = date(text);
            if forward { day.succ() } else { day.pred() }.map(|day| day.to_string())
        };
        assert_eq!(step("2026-10-21", true).as_deref(), Some("2026-10-22"));
        assert_eq!(step("2026-10-31", true).as_deref(), Some("2026-11-01"));
        assert_eq!(step("2026-12-31", true).as_deref(), Some("2027-01-01"));
        assert_eq!(step("2024-02-28", true).as_deref(), Some("2024-02-29"));
        assert_eq!(step("2024-02-29", true).as_deref(), Some("2024-03-01"));
        assert_eq!(step("2026-02-28", true).as_deref(), Some("2026-03-01"));
        assert_eq!(step("2026-03-01", false).as_deref(), Some("2026-02-28"));
        assert_eq!(step("2024-03-01", false).as_deref(), Some("2024-02-29"));
        assert_eq!(step("2027-01-01", false).as_deref(), Some("2026-12-31"));
        assert_eq!(step("2026-11-01", false).as_deref(), Some("2026-10-31"));
        assert_eq!(Date::MAX.succ(), None);
        assert_eq!(Date::MIN.pred(), None);
        assert_eq!(Date::new(7, 3, 9).unwrap().to_string(), "0007-03-09");
    }

    #[test]
    fn due_should_take_the_first_valid_token_anywhere_in_the_text() {
        fn day(text: &str) -> Option<String> {
            due(text).map(|date| date.to_string())
        }
        assert_eq!(day("due:2026-11-01 Pay").as_deref(), Some("2026-11-01"));
        assert_eq!(
            day("Pay due:2026-11-01 rent").as_deref(),
            Some("2026-11-01")
        );
        assert_eq!(
            day("Pay rent due:2026-11-01").as_deref(),
            Some("2026-11-01")
        );
        assert_eq!(
            day("(A) Pay rent due:2026-11-01. +home").as_deref(),
            Some("2026-11-01")
        );
        assert_eq!(
            day("x\tdue:2026-11-01\u{3000}y").as_deref(),
            Some("2026-11-01")
        );
        for mark in ['.', ',', ';', ':', '!', '?'] {
            let text = format!("x due:2026-11-01{mark}");
            assert_eq!(day(&text).as_deref(), Some("2026-11-01"), "{text:?}");
        }
        assert_eq!(day("x due:2026-11-01?!").as_deref(), Some("2026-11-01"));
        assert_eq!(
            day("due:2026-02-30 due:soon due:2026-11-01 due:2026-12-01").as_deref(),
            Some("2026-11-01")
        );
        for text in [
            "plain",
            "overdue:2026-11-01",
            "+due:2026-11-01",
            "Due:2026-11-01",
            "DUE:2026-11-01",
            "due:",
            "due: 2026-11-01",
            "due: x",
            "due:today",
            "due:2026-11-01T09:00",
            "due:2026-11-01)",
            "due:2026-1-5",
            "due:26-10-21",
            "due:2026/10/21",
            "due:2026-02-30",
            "due:2026-13-01",
            "due:２０２６-10-21",
            "xdue:2026-11-01",
        ] {
            assert_eq!(due(text), None, "{text:?}");
        }
    }

    #[test]
    fn due_token_slice_should_exclude_stripped_punctuation() {
        let text = "Pay due:2026-11-01.";
        let (slice, day) = due_token(text).unwrap();
        assert_eq!(slice, "due:2026-11-01");
        assert_eq!(day, date("2026-11-01"));
        assert_eq!(slice.as_ptr().addr() - text.as_ptr().addr(), "Pay ".len());
        assert_eq!(
            due_token("bad due:x due:2026-11-01!?").unwrap().0,
            "due:2026-11-01"
        );
        assert_eq!(due_token("no token"), None);
    }

    #[test]
    fn normalize_due_should_rewrite_relative_words_once_and_keep_other_bytes() {
        let reads = std::cell::Cell::new(0);
        let today = || {
            reads.set(reads.get() + 1);
            date("2026-10-21")
        };
        for text in [
            "plain",
            "x due:2026-11-01",
            "due:soon overdue:today Due:today",
        ] {
            assert!(matches!(normalize_due(text, today), Cow::Borrowed(t) if t == text));
        }
        assert_eq!(reads.get(), 0);

        assert_eq!(
            normalize_due("(A) Pay due:Today. +home", today),
            "(A) Pay due:2026-10-21. +home"
        );
        assert_eq!(reads.get(), 1);
        reads.set(0);
        let text = normalize_due("x due:today due:Tomorrow!", today);
        assert_eq!(text, "x due:2026-10-21 due:2026-10-22!");
        assert_eq!(reads.get(), 1);
        assert_eq!(due(&text), Some(date("2026-10-21")));
        assert_eq!(
            normalize_due("x\tdue:TOMORROW,\u{3000}y", today),
            "x\tdue:2026-10-22,\u{3000}y"
        );
        assert_eq!(
            normalize_due("overdue:today due:todays", today),
            "overdue:today due:todays"
        );

        assert_eq!(
            normalize_due("x due:tomorrow", || date("2026-10-31")),
            "x due:2026-11-01"
        );
        assert_eq!(
            normalize_due("x due:tomorrow", || date("2026-12-31")),
            "x due:2027-01-01"
        );
        assert_eq!(
            normalize_due("x due:tomorrow", || date("2024-02-28")),
            "x due:2024-02-29"
        );
        assert_eq!(
            normalize_due("x due:tomorrow", || Date::MAX),
            "x due:tomorrow"
        );
        assert_eq!(
            normalize_due("x due:tomorrow due:today", || Date::MAX),
            "x due:tomorrow due:9999-12-31"
        );
    }

    #[test]
    fn parse_due_term_should_accept_the_value_table_and_reject_the_rest() {
        let day = |text| Some(DueBound::Date(date(text)));
        let range = |start, end| Some(DueTerm::Range { start, end });
        assert_eq!(parse_due_term("none"), Some(DueTerm::None));
        assert_eq!(parse_due_term("None"), Some(DueTerm::None));
        assert_eq!(parse_due_term("ANY"), Some(DueTerm::Any));
        assert_eq!(parse_due_term("Overdue"), Some(DueTerm::Overdue));
        let today = Some(DueBound::Today);
        let tomorrow = Some(DueBound::Tomorrow);
        assert_eq!(parse_due_term("today"), range(today, today));
        assert_eq!(parse_due_term("TODAY"), range(today, today));
        assert_eq!(parse_due_term("Tomorrow"), range(tomorrow, tomorrow));
        assert_eq!(
            parse_due_term("2026-10-21"),
            range(day("2026-10-21"), day("2026-10-21"))
        );
        assert_eq!(
            parse_due_term("2026-10-01..2026-10-31"),
            range(day("2026-10-01"), day("2026-10-31"))
        );
        assert_eq!(
            parse_due_term("2026-10-21..2026-10-21"),
            range(day("2026-10-21"), day("2026-10-21"))
        );
        assert_eq!(
            parse_due_term("..2026-10-31"),
            range(None, day("2026-10-31"))
        );
        assert_eq!(parse_due_term("today.."), range(today, None));
        assert_eq!(parse_due_term("today..tomorrow"), range(today, tomorrow));
        assert_eq!(parse_due_term("today..today"), range(today, today));
        assert_eq!(
            parse_due_term("2026-10-25..today"),
            range(day("2026-10-25"), today)
        );
        assert_eq!(
            parse_due_term("today..2026-10-01"),
            range(today, day("2026-10-01"))
        );
        for value in [
            "",
            "..",
            "...",
            "soon",
            "week",
            "due:today",
            "2026-02-30",
            "2026-1-5",
            " today",
            "today ",
            "2026-10-22..2026-10-21",
            "tomorrow..today",
            "today...2026-11-01",
            "2026-10-01..2026-10-31..",
            "--json",
            "overdue..",
            "none..today",
        ] {
            assert_eq!(parse_due_term(value), None, "{value:?}");
        }

        let full = "today..2026-11-01";
        let parsed: Vec<usize> = (1..=full.len())
            .filter(|&end| parse_due_term(&full[..end]).is_some())
            .collect();
        assert_eq!(parsed, [5, 7, full.len()]);
    }

    #[test]
    fn due_filter_should_resolve_terms_lazily_and_fail_closed() {
        let empty = DueFilter::default();
        assert!(empty.is_empty());
        assert!(empty.matches(None, false));
        assert!(empty.matches(Some(date("2026-10-21")), true));

        let mut unresolved = DueFilter::default();
        unresolved.add(parse_due_term("any").unwrap());
        assert!(!unresolved.is_empty());
        assert!(!unresolved.matches(Some(date("2026-10-21")), false));
        assert!(!unresolved.matches(None, false));

        let mut absolute = DueFilter::default();
        for value in ["2026-10-21", "none", "any", "..2026-10-01", "2026-10-25.."] {
            absolute.add(parse_due_term(value).unwrap());
        }
        absolute.resolve(|| panic!("absolute terms must not read the clock"));
        assert!(absolute.matches(None, false));

        let mut duplicate = DueFilter::default();
        duplicate.add(parse_due_term("today").unwrap());
        duplicate.add(parse_due_term("TODAY").unwrap());
        assert_eq!(duplicate.terms.len(), 1);
        duplicate.resolve(|| date("2026-10-21"));
        duplicate.add(parse_due_term("tomorrow").unwrap());
        assert_eq!(duplicate.spans, None, "add clears resolved spans");
    }

    #[test]
    fn due_filter_should_union_terms_and_keep_overdue_open_only() {
        let at = |text| Some(date(text));
        let union = resolved(&["overdue", "today", "tomorrow"], "2026-10-21");
        assert!(union.matches(at("2026-10-20"), false));
        assert!(!union.matches(at("2026-10-20"), true));
        assert!(union.matches(at("2026-10-21"), false));
        assert!(union.matches(at("2026-10-21"), true));
        assert!(union.matches(at("2026-10-22"), true));
        assert!(!union.matches(at("2026-10-23"), false));
        assert!(!union.matches(None, false));

        let overdue = resolved(&["overdue"], "2026-10-21");
        assert!(overdue.matches(at("2000-01-01"), false));
        assert!(!overdue.matches(at("2026-10-21"), false));
        assert!(!overdue.matches(at("2026-10-20"), true));

        let none = resolved(&["none"], "2026-10-21");
        assert!(none.matches(None, false));
        assert!(!none.matches(at("2026-10-21"), false));
        let any = resolved(&["any"], "2026-10-21");
        assert!(any.matches(at("0000-01-01"), true));
        assert!(any.matches(at("9999-12-31"), false));
        assert!(!any.matches(None, false));

        let range = resolved(&["2026-10-01..2026-10-31"], "2026-10-21");
        assert!(range.matches(at("2026-10-01"), false));
        assert!(range.matches(at("2026-10-31"), true));
        assert!(!range.matches(at("2026-09-30"), false));
        assert!(!range.matches(at("2026-11-01"), false));
        let open_end = resolved(&["today.."], "2026-10-21");
        assert!(open_end.matches(at("9999-12-31"), false));
        assert!(!open_end.matches(at("2026-10-20"), false));
        let open_start = resolved(&["..tomorrow"], "2026-10-21");
        assert!(open_start.matches(at("0000-01-01"), false));
        assert!(open_start.matches(at("2026-10-22"), false));
        assert!(!open_start.matches(at("2026-10-23"), false));

        let mixed = resolved(&["2026-10-25..today"], "2026-10-21");
        assert!(!mixed.matches(at("2026-10-21"), false));
        assert!(!mixed.matches(at("2026-10-25"), false));
        let mixed_later = resolved(&["2026-10-25..today"], "2026-10-27");
        assert!(mixed_later.matches(at("2026-10-26"), false));

        let first_day = resolved(&["overdue"], "0000-01-01");
        assert!(!first_day.matches(at("0000-01-01"), false));
        assert!(!first_day.matches(at("9999-12-31"), false));
        let last_day = resolved(&["tomorrow"], "9999-12-31");
        assert!(!last_day.matches(at("9999-12-31"), false));

        let mut moving = resolved(&["today", "overdue"], "2026-10-21");
        assert!(moving.matches(at("2026-10-21"), false));
        moving.resolve(|| date("2026-10-22"));
        assert!(moving.matches(at("2026-10-21"), false), "now overdue");
        assert!(moving.matches(at("2026-10-22"), false));
        assert!(!moving.matches(at("2026-10-21"), true));
    }

    #[test]
    fn task_filter_should_and_due_with_tags_and_priorities() {
        let mut combined = filter(&["+home", "A"]);
        combined.due.add(parse_due_term("overdue").unwrap());
        combined.due.add(parse_due_term("today").unwrap());
        assert!(!combined.is_empty());
        combined.due.resolve(|| date("2026-10-21"));
        assert!(combined.matches("(A) Pay due:2026-10-20 +home", false));
        assert!(combined.matches("(A) Pay due:2026-10-21. +home", true));
        assert!(!combined.matches("(A) Pay due:2026-10-20 +home", true));
        assert!(!combined.matches("(B) Pay due:2026-10-21 +home", false));
        assert!(!combined.matches("(A) Pay due:2026-10-21 +work", false));
        assert!(!combined.matches("(A) Pay due:2026-10-22 +home", false));
        assert!(!combined.matches("(A) Pay +home", false));

        let mut only_due = TaskFilter::default();
        only_due.due.add(parse_due_term("none").unwrap());
        assert!(!only_due.is_empty());
        only_due.due.resolve(|| unreachable!());
        assert!(only_due.matches("plain due:2026-02-30", false));
        assert!(!only_due.matches("dated due:2026-02-28", false));

        let no_due = filter(&["+home"]);
        assert!(no_due.due.is_empty());
        assert!(no_due.matches("x +home due:2026-10-20", true));
        assert!(no_due.matches("x +home due:bad", false));
    }
}
