//! `java.util.regex` (`Pattern`, `Matcher`, `PatternSyntaxException`) as JDK 21 behaves, on top of fancy-regex.
//!
//! A Java pattern is parsed here and written out as a fancy-regex pattern in which every Java rule is spelled out
//! explicitly: line terminators for `.`, `^`, `$` and `\Z`; `\b` over Java's word characters; character classes as
//! explicit code point sets (union, `&&` intersection and negation computed here); case-insensitive matching as
//! explicit case variants; Unicode properties from tables recorded in JDK 21 (`tables`). fancy-regex then only
//! runs a backtracking search, which has the same leftmost-first semantics as Java's matcher.
//!
//! Written from the Java SE 21 API specification of `Pattern`, `Matcher` and `PatternSyntaxException` and from
//! behaviour observed in JDK 21 (`docs/provenance.md`), not from OpenJDK source.
//!
//! Positions in [`Match`] are byte offsets into the Rust string; error indexes in [`SyntaxError`] are UTF-16
//! indexes into the pattern, as Java reports them.

#[rustfmt::skip]
mod tables;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use fancy_regex::Regex;
use parking_lot::Mutex;

use crate::lang::JError;

/// `Pattern.UNIX_LINES`.
pub const UNIX_LINES: u32 = 0x01;
/// `Pattern.CASE_INSENSITIVE`.
pub const CASE_INSENSITIVE: u32 = 0x02;
/// `Pattern.COMMENTS`.
pub const COMMENTS: u32 = 0x04;
/// `Pattern.MULTILINE`.
pub const MULTILINE: u32 = 0x08;
/// `Pattern.LITERAL`.
pub const LITERAL: u32 = 0x10;
/// `Pattern.DOTALL`.
pub const DOTALL: u32 = 0x20;
/// `Pattern.UNICODE_CASE`.
pub const UNICODE_CASE: u32 = 0x40;
/// `Pattern.CANON_EQ`.
pub const CANON_EQ: u32 = 0x80;
/// `Pattern.UNICODE_CHARACTER_CLASS`.
pub const UNICODE_CHARACTER_CLASS: u32 = 0x100;

/// `java.util.regex.PatternSyntaxException`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// `getDescription()`.
    pub desc: String,
    /// The UTF-16 index in the pattern the error is reported at, or -1.
    pub index: i64,
    /// `getPattern()`.
    pub pattern: String,
}

impl SyntaxError {
    /// `getMessage()`: the description, ` near index N` when there is an index, the pattern on the next line and a
    /// caret under the index when the index falls inside the pattern.
    pub fn message(&self) -> String {
        let mut m = self.desc.clone();
        if self.index >= 0 {
            m.push_str(&format!(" near index {}", self.index));
        }
        m.push('\n');
        m.push_str(&self.pattern);
        if self.index >= 0 && (self.index as usize) < self.pattern.encode_utf16().count() {
            m.push('\n');
            m.push_str(&" ".repeat(self.index as usize));
            m.push('^');
        }
        m
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for SyntaxError {}

impl From<SyntaxError> for JError {
    fn from(e: SyntaxError) -> JError {
        JError::new("java.util.regex.PatternSyntaxException", e.message())
    }
}

/// One successful match: the byte span of every group, group 0 first; `None` for a group that took no part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Each group's byte span, group 0 (the whole match) first.
    pub groups: Vec<Option<(usize, usize)>>,
}

impl Match {
    /// `start()`: the match's first byte.
    pub fn start(&self) -> usize {
        self.groups[0].map_or(0, |g| g.0)
    }

    /// `end()`: the byte after the match.
    pub fn end(&self) -> usize {
        self.groups[0].map_or(0, |g| g.1)
    }

    /// `Matcher.group(i)` against the input `s` the match was made on; `None` for a group that took no part or does
    /// not exist.
    pub fn group<'a>(&self, s: &'a str, i: usize) -> Option<&'a str> {
        self.groups.get(i).copied().flatten().map(|(a, b)| &s[a..b])
    }
}

/// A compiled `java.util.regex.Pattern`.
#[derive(Debug)]
pub struct Pattern {
    source: String,
    flags: u32,
    /// The translated pattern, unanchored (`Matcher.find`).
    text: String,
    find: Regex,
    whole: OnceLock<Result<Regex, String>>,
    prefix: OnceLock<Result<Regex, String>>,
    group_count: usize,
    names: HashMap<String, usize>,
}

const CACHE_LIMIT: usize = 1024;

/// Compiled patterns by (pattern, flags).
type Cache = Mutex<HashMap<(String, u32), Arc<Pattern>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A fancy-regex failure while matching (its backtracking limit): Java would have run out of stack instead.
fn runtime(e: fancy_regex::Error) -> JError {
    JError::new("java.lang.StackOverflowError", e.to_string())
}

impl Pattern {
    /// `Pattern.compile(pattern, flags)`, shared: patterns are compiled once per (pattern, flags).
    pub fn get(pattern: &str, flags: u32) -> Result<Arc<Pattern>, SyntaxError> {
        let key = (pattern.to_string(), flags);
        if let Some(p) = cache().lock().get(&key) {
            return Ok(p.clone());
        }
        let p = Arc::new(Pattern::compile(pattern, flags)?);
        let mut c = cache().lock();
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(key, p.clone());
        Ok(p)
    }

    /// `Pattern.compile(pattern, flags)`.
    pub fn compile(pattern: &str, flags: u32) -> Result<Pattern, SyntaxError> {
        let mut parser = Parser::new(pattern, flags);
        let text = if flags & LITERAL != 0 {
            parser.literal_pattern()
        } else {
            parser.parse()?
        };
        let text = parser.resolve_backrefs(&text);
        let find = Regex::new(&text).map_err(|e| SyntaxError {
            desc: format!("Pattern not supported by java-compat-rs: {e}"),
            index: -1,
            pattern: pattern.to_string(),
        })?;
        Ok(Pattern {
            source: pattern.to_string(),
            flags,
            text,
            find,
            whole: OnceLock::new(),
            prefix: OnceLock::new(),
            group_count: parser.groups,
            names: parser.names,
        })
    }

    /// `Pattern.pattern()`.
    pub fn pattern(&self) -> &str {
        &self.source
    }

    /// `Pattern.flags()`.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// The fancy-regex this pattern translates to; searching it behaves as `Matcher.find`.
    pub fn regex(&self) -> &Regex {
        &self.find
    }

    /// `Matcher.groupCount()`.
    pub fn group_count(&self) -> usize {
        self.group_count
    }

    /// The number of the named group `name`, if the pattern has one.
    pub fn group_index(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    fn anchored<'a>(
        &'a self,
        cell: &'a OnceLock<Result<Regex, String>>,
        end: &str,
    ) -> Result<&'a Regex, JError> {
        cell.get_or_init(|| {
            Regex::new(&format!("\\A(?:{}){end}", self.text)).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| JError::new("java.lang.UnsupportedOperationException", e.clone()))
    }

    fn to_match(&self, c: Option<fancy_regex::Captures<'_, str>>) -> Option<Match> {
        c.map(|c| Match {
            groups: (0..=self.group_count)
                .map(|i| c.get(i).map(|m| (m.start(), m.end())))
                .collect(),
        })
    }

    /// `Matcher.matches()`: the whole input.
    pub fn matches(&self, s: &str) -> Result<Option<Match>, JError> {
        let re = self.anchored(&self.whole, "\\z")?;
        Ok(self.to_match(re.captures(s).map_err(runtime)?))
    }

    /// `Matcher.lookingAt()`: a prefix of the input.
    pub fn looking_at(&self, s: &str) -> Result<Option<Match>, JError> {
        let re = self.anchored(&self.prefix, "")?;
        Ok(self.to_match(re.captures(s).map_err(runtime)?))
    }

    /// `Matcher.find(from)`: the first match starting at byte `from` or later (look-behind sees the text before it).
    pub fn find_at(&self, s: &str, from: usize) -> Result<Option<Match>, JError> {
        if from > s.len() {
            return Ok(None);
        }
        Ok(self.to_match(self.find.captures_from_pos(s, from).map_err(runtime)?))
    }

    /// Every match `Matcher.find()` returns in turn: after an empty match the search resumes one character later.
    pub fn find_all(&self, s: &str) -> Result<Vec<Match>, JError> {
        let mut all = Vec::new();
        let mut from = 0;
        while let Some(m) = self.find_at(s, from)? {
            from = if m.start() == m.end() {
                next_char(s, m.end())
            } else {
                m.end()
            };
            all.push(m);
        }
        Ok(all)
    }

    /// `Matcher.replaceAll(replacement)` (`all`) or `replaceFirst(replacement)`.
    pub fn replace(&self, s: &str, replacement: &str, all: bool) -> Result<String, JError> {
        let matches = if all {
            self.find_all(s)?
        } else {
            self.find_at(s, 0)?.into_iter().collect()
        };
        let mut out = String::with_capacity(s.len());
        let mut last = 0;
        for m in &matches {
            out.push_str(&s[last..m.start()]);
            self.append_replacement(s, m, replacement, &mut out)?;
            last = m.end();
        }
        out.push_str(&s[last..]);
        Ok(out)
    }

    /// The replacement part of `Matcher.appendReplacement`: `replacement` with `$n`, `${name}` and `\` escapes
    /// expanded against `m`, appended to `out`.
    pub fn append_replacement(
        &self,
        s: &str,
        m: &Match,
        replacement: &str,
        out: &mut String,
    ) -> Result<(), JError> {
        expand(
            replacement,
            self.group_count,
            |name| self.group_index(name).map(|i| m.group(s, i)).ok_or(()),
            |i| m.group(s, i),
            out,
        )
    }

    /// `Pattern.split(input, limit)`: a positive limit allows `limit - 1` splits, zero drops trailing empty strings,
    /// a negative limit keeps them; a zero-width match at the very start never yields a leading empty string.
    pub fn split(&self, s: &str, limit: i32) -> Result<Vec<String>, JError> {
        let mut parts: Vec<String> = Vec::new();
        let mut index = 0;
        let mut split = false;
        let mut from = 0;
        while let Some(m) = self.find_at(s, from)? {
            from = if m.start() == m.end() {
                next_char(s, m.end())
            } else {
                m.end()
            };
            if limit > 0 && parts.len() as i64 >= limit as i64 - 1 {
                break;
            }
            if m.end() == 0 {
                continue;
            }
            parts.push(s[index..m.start()].to_string());
            index = m.end();
            split = true;
        }
        if !split {
            return Ok(vec![s.to_string()]);
        }
        parts.push(s[index..].to_string());
        if limit == 0 {
            while parts.last().is_some_and(|p| p.is_empty()) {
                parts.pop();
            }
        }
        Ok(parts)
    }
}

/// The replacement-string syntax of `Matcher.appendReplacement`: `\x` is `x`; `$n` is group `n`, taking further
/// digits while the number stays a group of the pattern; `${name}` is a named group. `name` answers `Err(())` for
/// a name the pattern lacks and `Ok(None)` for a group that took no part.
pub fn expand<'a>(
    replacement: &str,
    group_count: usize,
    name: impl Fn(&str) -> Result<Option<&'a str>, ()>,
    group: impl Fn(usize) -> Option<&'a str>,
    out: &mut String,
) -> Result<(), JError> {
    let illegal = |m: String| JError::new("java.lang.IllegalArgumentException", m);
    let text: Vec<char> = replacement.chars().collect();
    let mut at = 0;
    while at < text.len() {
        match text[at] {
            '\\' => {
                let Some(&c) = text.get(at + 1) else {
                    return Err(illegal("character to be escaped is missing".into()));
                };
                out.push(c);
                at += 2;
            }
            '$' => {
                at += 1;
                let Some(&c) = text.get(at) else {
                    return Err(illegal(
                        "Illegal group reference: group index is missing".into(),
                    ));
                };
                if c == '{' {
                    at += 1;
                    let begin = at;
                    while text.get(at).is_some_and(|c| c.is_ascii_alphanumeric()) {
                        at += 1;
                    }
                    let id: String = text[begin..at].iter().collect();
                    if id.is_empty() {
                        return Err(illegal("named capturing group has 0 length name".into()));
                    }
                    if text.get(at) != Some(&'}') {
                        return Err(illegal(
                            "named capturing group is missing trailing '}'".into(),
                        ));
                    }
                    at += 1;
                    if id.starts_with(|c: char| c.is_ascii_digit()) {
                        return Err(illegal(format!(
                            "capturing group name {{{id}}} starts with digit character"
                        )));
                    }
                    match name(&id) {
                        Err(()) => return Err(illegal(format!("No group with name {{{id}}}"))),
                        Ok(value) => out.push_str(value.unwrap_or("")),
                    }
                } else {
                    let Some(d) = c.to_digit(10) else {
                        return Err(illegal("Illegal group reference".into()));
                    };
                    let mut number = d as usize;
                    if number > group_count {
                        return Err(JError::new(
                            "java.lang.IndexOutOfBoundsException",
                            format!("No group {number}"),
                        ));
                    }
                    at += 1;
                    while let Some(d) = text.get(at).and_then(|c| c.to_digit(10)) {
                        let longer = number * 10 + d as usize;
                        if longer > group_count {
                            break;
                        }
                        number = longer;
                        at += 1;
                    }
                    out.push_str(group(number).unwrap_or(""));
                }
            }
            c => {
                out.push(c);
                at += 1;
            }
        }
    }
    Ok(())
}

/// `Pattern.quote(s)`: `s` between `\Q` and `\E`, any `\E` inside it closed, escaped and reopened.
pub fn quote(s: &str) -> String {
    format!("\\Q{}\\E", s.replace("\\E", "\\E\\\\E\\Q"))
}

/// `Matcher.quoteReplacement(s)`: `\` and `$` escaped with a backslash.
pub fn quote_replacement(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\\' || c == '$' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The byte index after the character at byte `i` of `s`, or `i + 1` at (or past) the end.
pub fn next_char(s: &str, i: usize) -> usize {
    match s.get(i..).and_then(|rest| rest.chars().next()) {
        Some(c) => i + c.len_utf8(),
        None => i + 1,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Code point sets.

const MAX: u32 = 0x10FFFF;

/// A set of code points: sorted, disjoint, non-adjacent inclusive ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Set(Vec<(u32, u32)>);

impl Set {
    fn all() -> Set {
        Set(vec![(0, MAX)])
    }

    fn of(c: u32) -> Set {
        Set(vec![(c, c)])
    }

    fn from(mut ranges: Vec<(u32, u32)>) -> Set {
        ranges.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (a, b) in ranges {
            match out.last_mut() {
                Some(last) if a <= last.1.saturating_add(1) => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
        }
        Set(out)
    }

    fn chars(s: &str) -> Set {
        Set::from(s.chars().map(|c| (c as u32, c as u32)).collect())
    }

    fn union(&self, o: &Set) -> Set {
        let mut v = self.0.clone();
        v.extend_from_slice(&o.0);
        Set::from(v)
    }

    fn intersect(&self, o: &Set) -> Set {
        let (mut i, mut j, mut out) = (0, 0, Vec::new());
        while i < self.0.len() && j < o.0.len() {
            let (a, b) = self.0[i];
            let (c, d) = o.0[j];
            let (lo, hi) = (a.max(c), b.min(d));
            if lo <= hi {
                out.push((lo, hi));
            }
            if b < d {
                i += 1;
            } else {
                j += 1;
            }
        }
        Set(out)
    }

    fn negate(&self) -> Set {
        let mut out = Vec::new();
        let mut next = 0u32;
        for &(a, b) in &self.0 {
            if a > next {
                out.push((next, a - 1));
            }
            next = b + 1;
        }
        if next <= MAX {
            out.push((next, MAX));
        }
        Set(out)
    }

    fn minus(&self, o: &Set) -> Set {
        self.intersect(&o.negate())
    }

    fn contains(&self, c: u32) -> bool {
        self.0
            .binary_search_by(|&(a, b)| {
                if b < c {
                    std::cmp::Ordering::Less
                } else if a > c {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    /// The code points of runs `(start, end, key)` whose key is accepted.
    fn runs(runs: &[(u32, u32, u16)], accept: impl Fn(u16) -> bool) -> Set {
        Set::from(
            runs.iter()
                .filter(|r| accept(r.2))
                .map(|r| (r.0, r.1))
                .collect(),
        )
    }

    fn ranges(r: &[(u32, u32)]) -> Set {
        Set(r.to_vec())
    }
}

/// A code point in fancy-regex syntax.
fn lit(c: u32) -> String {
    match char::from_u32(c) {
        Some(ch) if ch.is_ascii_alphanumeric() => ch.to_string(),
        _ => format!("\\x{{{c:X}}}"),
    }
}

/// The fancy-regex text that matches nothing.
const NEVER: &str = "(?!)";

/// A set in fancy-regex syntax. Surrogate code points cannot occur in a Rust string and are left out.
fn emit_set(s: &Set) -> String {
    let s = s.minus(&Set(vec![(0xD800, 0xDFFF)]));
    match s.0.as_slice() {
        [] => NEVER.into(),
        [(0, 0xD7FF), (0xE000, MAX)] => "(?s:.)".into(),
        [(a, b)] if a == b => lit(*a),
        ranges => {
            let mut out = String::from("[");
            for &(a, b) in ranges {
                out.push_str(&lit(a));
                if b > a {
                    out.push('-');
                    out.push_str(&lit(b));
                }
            }
            out.push(']');
            out
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Unicode data (JDK 21) and the predefined classes.

fn category(name: &str) -> Option<Set> {
    let gc = |accept: &dyn Fn(&str) -> bool| {
        Set::runs(tables::GC, |k| accept(tables::GC_NAMES[k as usize]))
    };
    Some(match name {
        "all" => Set::all(),
        "L1" => Set(vec![(0, 0xFF)]),
        "LC" => gc(&|n| matches!(n, "Lu" | "Ll" | "Lt")),
        "LD" => gc(&|n| n.starts_with('L') || n == "Nd"),
        "L" | "M" | "N" | "Z" | "C" | "P" | "S" => gc(&|n| n.starts_with(name)),
        _ if tables::GC_NAMES.contains(&name) => gc(&|n| n == name),
        _ => return None,
    })
}

fn gc(name: &str) -> Set {
    category(name).expect("a general category")
}

fn prop(name: &str) -> Set {
    Set::ranges(
        tables::PROPS
            .iter()
            .find(|p| p.0 == name)
            .expect("a recorded property")
            .1,
    )
}

const ASCII_WORD: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";

fn ascii_range(a: char, b: char) -> Set {
    Set(vec![(a as u32, b as u32)])
}

/// Unicode `\w` (`UNICODE_CHARACTER_CLASS`): alphabetic, marks, decimal digits, connector punctuation, join control.
fn unicode_word() -> Set {
    ["Mn", "Me", "Mc", "Nd", "Pc"].iter().fold(
        prop("IsAlphabetic").union(&prop("IsJoin_Control")),
        |s, g| s.union(&gc(g)),
    )
}

/// The POSIX classes: US-ASCII, or their Unicode definitions under `UNICODE_CHARACTER_CLASS`.
fn posix(name: &str, unicode: bool) -> Option<Set> {
    if unicode {
        return Some(match name {
            "Lower" => prop("IsLowercase"),
            "Upper" => prop("IsUppercase"),
            "ASCII" => Set(vec![(0, 0x7F)]),
            "Alpha" => prop("IsAlphabetic"),
            "Digit" => gc("Nd"),
            "Alnum" => prop("IsAlphabetic").union(&gc("Nd")),
            "Punct" => gc("P"),
            "Graph" => unicode_graph(),
            "Print" => unicode_graph().union(&unicode_blank()).minus(&gc("Cc")),
            "Blank" => unicode_blank(),
            "Cntrl" => gc("Cc"),
            "XDigit" => gc("Nd").union(&prop("IsHex_Digit")),
            "Space" => prop("IsWhite_Space"),
            _ => return None,
        });
    }
    let punct = ascii_range('!', '/')
        .union(&ascii_range(':', '@'))
        .union(&ascii_range('[', '`'))
        .union(&ascii_range('{', '~'));
    let alnum = Set::chars(ASCII_WORD).minus(&Set::of('_' as u32));
    Some(match name {
        "Lower" => ascii_range('a', 'z'),
        "Upper" => ascii_range('A', 'Z'),
        "ASCII" => Set(vec![(0, 0x7F)]),
        "Alpha" => ascii_range('a', 'z').union(&ascii_range('A', 'Z')),
        "Digit" => ascii_range('0', '9'),
        "Alnum" => alnum,
        "Punct" => punct,
        "Graph" => alnum.union(&punct),
        "Print" => alnum.union(&punct).union(&Set::of(' ' as u32)),
        "Blank" => Set::chars(" \t"),
        "Cntrl" => Set(vec![(0, 0x1F), (0x7F, 0x7F)]),
        "XDigit" => ascii_range('0', '9')
            .union(&ascii_range('a', 'f'))
            .union(&ascii_range('A', 'F')),
        "Space" => Set::chars(" \t\n\x0B\x0C\r"),
        _ => return None,
    })
}

fn unicode_graph() -> Set {
    prop("IsWhite_Space")
        .union(&gc("Cc"))
        .union(&gc("Cs"))
        .union(&gc("Cn"))
        .negate()
}

fn unicode_blank() -> Set {
    prop("IsWhite_Space").minus(
        &gc("Zl")
            .union(&gc("Zp"))
            .union(&Set::chars("\n\x0B\x0C\r\u{85}")),
    )
}

/// `\p{IsX}` binary properties; `X` is matched case-insensitively.
fn binary(upper: &str) -> Option<Set> {
    Some(match upper {
        "ALPHABETIC" | "ALPHA" => prop("IsAlphabetic"),
        "LETTER" => gc("L"),
        "IDEOGRAPHIC" => prop("IsIdeographic"),
        "LOWERCASE" | "LOWER" => prop("IsLowercase"),
        "UPPERCASE" | "UPPER" => prop("IsUppercase"),
        "TITLECASE" => gc("Lt"),
        "WHITE_SPACE" | "WHITESPACE" | "SPACE" => prop("IsWhite_Space"),
        "CONTROL" | "CNTRL" => gc("Cc"),
        "PUNCTUATION" | "PUNCT" => gc("P"),
        "HEX_DIGIT" | "HEXDIGIT" => prop("IsHex_Digit"),
        "XDIGIT" => posix("XDigit", true)?,
        "ASSIGNED" => gc("Cn").negate(),
        "NONCHARACTER_CODE_POINT" | "NONCHARACTERCODEPOINT" => prop("IsNoncharacter_Code_Point"),
        "DIGIT" => gc("Nd"),
        "ALNUM" => posix("Alnum", true)?,
        "BLANK" => unicode_blank(),
        "GRAPH" => unicode_graph(),
        "PRINT" => posix("Print", true)?,
        "WORD" => unicode_word(),
        "JOIN_CONTROL" | "JOINCONTROL" => prop("IsJoin_Control"),
        "EMOJI" => prop("IsEmoji"),
        "EMOJI_PRESENTATION" => prop("IsEmoji_Presentation"),
        "EMOJI_MODIFIER" => prop("IsEmoji_Modifier"),
        "EMOJI_MODIFIER_BASE" => prop("IsEmoji_Modifier_Base"),
        "EMOJI_COMPONENT" => prop("IsEmoji_Component"),
        "EXTENDED_PICTOGRAPHIC" => prop("IsExtended_Pictographic"),
        _ => return None,
    })
}

/// The `java.lang.Character` predicates (`\p{javaLowerCase}` ...).
fn java_class(name: &str) -> Option<Set> {
    Some(match name {
        "javaLowerCase"
        | "javaUpperCase"
        | "javaAlphabetic"
        | "javaIdeographic"
        | "javaMirrored"
        | "javaWhitespace"
        | "javaUnicodeIdentifierStart"
        | "javaUnicodeIdentifierPart"
        | "javaIdentifierIgnorable"
        | "javaJavaIdentifierStart"
        | "javaJavaIdentifierPart" => prop(name),
        "javaTitleCase" => gc("Lt"),
        "javaLetter" => gc("L"),
        "javaDigit" => gc("Nd"),
        "javaLetterOrDigit" => gc("L").union(&gc("Nd")),
        "javaDefined" => gc("Cn").negate(),
        "javaSpaceChar" => gc("Zs").union(&gc("Zl")).union(&gc("Zp")),
        "javaISOControl" => Set(vec![(0, 0x1F), (0x7F, 0x9F)]),
        _ => return None,
    })
}

fn script(name: &str) -> Option<Set> {
    let upper = name.to_ascii_uppercase();
    let index = match tables::SCRIPT_NAMES.iter().position(|n| *n == upper) {
        Some(i) => i as u16,
        None => tables::SCRIPT_ALIASES.iter().find(|a| a.0 == upper)?.1,
    };
    Some(Set::runs(tables::SCRIPTS, |k| k == index))
}

fn block(name: &str) -> Option<Set> {
    let lower = name.to_lowercase();
    let index = tables::BLOCK_NAMES
        .iter()
        .position(|names| names.contains(&lower.as_str()))? as u16;
    Some(Set::runs(tables::BLOCKS, |k| k == index))
}

/// How a property reacts to `CASE_INSENSITIVE`: the case categories become all cased letters, the case properties
/// all characters with any case, the ASCII `Lower`/`Upper` both ASCII cases; everything else is unchanged.
#[derive(Clone, Copy, PartialEq)]
enum CaseRule {
    Fixed,
    CaseCategory,
    CaseProperty,
    AsciiCase,
}

fn case_rule_of_category(name: &str) -> CaseRule {
    if matches!(name, "Lu" | "Ll" | "Lt") {
        CaseRule::CaseCategory
    } else {
        CaseRule::Fixed
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Case-insensitive matching.

fn flag(flags: u32, f: u32) -> bool {
    flags & f != 0
}

fn unicode_case(flags: u32) -> bool {
    flags & (UNICODE_CASE | UNICODE_CHARACTER_CLASS) != 0
}

/// The code points a literal `c` matches under `flags`.
fn literal_set(c: u32, flags: u32) -> Set {
    if !flag(flags, CASE_INSENSITIVE) {
        return Set::of(c);
    }
    if unicode_case(flags) {
        let mut v = vec![(c, c)];
        if let Ok(i) = tables::FOLD_LITERAL.binary_search_by_key(&c, |e| e.0) {
            v.extend(tables::FOLD_LITERAL[i].1.iter().map(|&d| (d, d)));
        }
        return Set::from(v);
    }
    ascii_fold(&Set::of(c))
}

/// `s` with the ASCII letters' other case added.
fn ascii_fold(s: &Set) -> Set {
    let lower = s
        .intersect(&ascii_range('a', 'z'))
        .0
        .iter()
        .map(|&(a, b)| (a - 32, b - 32))
        .collect::<Vec<_>>();
    let upper = s
        .intersect(&ascii_range('A', 'Z'))
        .0
        .iter()
        .map(|&(a, b)| (a + 32, b + 32))
        .collect::<Vec<_>>();
    s.union(&Set::from(lower)).union(&Set::from(upper))
}

/// The code points the characters of `s`, each a single-character class member, match under `flags`.
fn class_fold(s: &Set, flags: u32) -> Set {
    if !flag(flags, CASE_INSENSITIVE) {
        return s.clone();
    }
    if unicode_case(flags) {
        let mut v = s.0.clone();
        for (c, others) in tables::FOLD_CLASS {
            if s.contains(*c) {
                v.extend(others.iter().map(|&d| (d, d)));
            }
        }
        return Set::from(v);
    }
    ascii_fold(s)
}

fn to_lower(c: u32) -> u32 {
    tables::CASE_MAP
        .binary_search_by_key(&c, |e| e.0)
        .map_or(c, |i| tables::CASE_MAP[i].2)
}

/// The code points the class range `lo-hi` matches under `flags`: with Unicode case folding, a character whose
/// upper case, or the lower case of that, falls in the range.
fn range_fold(lo: u32, hi: u32, flags: u32) -> Set {
    let range = Set(vec![(lo, hi)]);
    if !flag(flags, CASE_INSENSITIVE) {
        return range;
    }
    if !unicode_case(flags) {
        return ascii_fold(&range);
    }
    let within = |c: u32| (lo..=hi).contains(&c);
    let mut v = range.0.clone();
    for &(c, upper, _) in tables::CASE_MAP {
        if within(upper) || within(to_lower(upper)) {
            v.push((c, c));
        }
    }
    Set::from(v)
}

fn apply_case_rule(s: Set, rule: CaseRule, flags: u32) -> Set {
    if !flag(flags, CASE_INSENSITIVE) {
        return s;
    }
    match rule {
        CaseRule::Fixed => s,
        CaseRule::CaseCategory => gc("LC"),
        CaseRule::CaseProperty => prop("IsLowercase")
            .union(&prop("IsUppercase"))
            .union(&gc("Lt")),
        CaseRule::AsciiCase => ascii_range('a', 'z').union(&ascii_range('A', 'Z')),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The parser: Java syntax in, fancy-regex syntax out.

/// What an escape sequence stands for.
enum Escape {
    Char(u32),
    Set(Set),
    /// A multi-character construct or back reference, already in fancy-regex syntax.
    Node(String),
    /// A zero-width assertion, already in fancy-regex syntax.
    Anchor(String),
    /// `\Q`: the caller reads the quoted text.
    Quote,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// An atom a `*`, `+`, `?` or `{n}` may follow.
    Atom,
    /// Quantified with `*`, `+` or `?`: only `{n}` may follow.
    Starred,
    /// Quantified with `{n}`: another `{n}` may follow.
    Counted,
    /// An inline flag group, which nothing may quantify.
    Flags,
}

struct Piece {
    text: String,
    state: State,
    /// Matches only the empty string (an anchor or look-around): fancy-regex cannot repeat it.
    zero_width: bool,
}

/// Marks a numbered back reference in the output until the group count is known: `BACKREF n BACKREF c` (`c` is 1
/// under case-insensitive matching). No other raw non-ASCII character is ever written to the output.
const BACKREF: char = '\u{F8FF}';

struct Parser {
    source: String,
    chars: Vec<char>,
    utf16: Vec<usize>,
    pos: usize,
    flags: u32,
    groups: usize,
    names: HashMap<String, usize>,
}

const LINE_TERMINATORS: &str = "\n\r\u{85}\u{2028}\u{2029}";

impl Parser {
    fn new(pattern: &str, flags: u32) -> Parser {
        let chars: Vec<char> = pattern.chars().collect();
        let mut utf16 = Vec::with_capacity(chars.len() + 1);
        let mut n = 0;
        for c in &chars {
            utf16.push(n);
            n += c.len_utf16();
        }
        utf16.push(n);
        Parser {
            source: pattern.to_string(),
            chars,
            utf16,
            pos: 0,
            flags,
            groups: 0,
            names: HashMap::new(),
        }
    }

    fn err(&self, desc: impl Into<String>, at: isize) -> SyntaxError {
        let index = if at < 0 {
            -1
        } else {
            self.utf16[(at as usize).min(self.chars.len())] as i64
        };
        SyntaxError {
            desc: desc.into(),
            index,
            pattern: self.source.clone(),
        }
    }

    fn len(&self) -> isize {
        self.chars.len() as isize
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    fn here(&self) -> isize {
        self.pos as isize
    }

    /// In `COMMENTS` mode: whitespace and `#` comments up to the end of the line.
    fn skip_comments(&mut self) {
        if !flag(self.flags, COMMENTS) {
            return;
        }
        while let Some(c) = self.peek() {
            if matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r') {
                self.pos += 1;
            } else if c == '#' {
                let end = self.terminators();
                while let Some(c) = self.peek() {
                    self.pos += 1;
                    if end.contains(c) {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn literal_pattern(&mut self) -> String {
        self.chars
            .iter()
            .map(|&c| emit_set(&literal_set(c as u32, self.flags)))
            .collect()
    }

    fn parse(&mut self) -> Result<String, SyntaxError> {
        let out = self.alternation()?;
        if self.pos < self.chars.len() {
            // Only an unmatched ')' stops the top level early.
            return Err(self.err("Unmatched closing ')'", self.here() - 1));
        }
        Ok(out)
    }

    /// Numbered back references become fancy-regex ones, or never match when the group does not exist.
    fn resolve_backrefs(&self, text: &str) -> String {
        if !text.contains(BACKREF) {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut parts = text.split(BACKREF);
        out.push_str(parts.next().unwrap_or(""));
        while let (Some(number), Some(rest)) = (parts.next(), parts.next()) {
            let n: usize = number.parse().unwrap_or(0);
            let (ci, rest) = rest.split_at(1);
            if n == 0 || n > self.groups {
                out.push_str("(?!)");
            } else if ci == "1" {
                out.push_str(&format!("(?i:\\k<{n}>)"));
            } else {
                out.push_str(&format!("\\k<{n}>"));
            }
            out.push_str(rest);
        }
        out
    }

    fn alternation(&mut self) -> Result<String, SyntaxError> {
        let mut out = self.sequence()?;
        while self.peek() == Some('|') {
            self.pos += 1;
            out.push('|');
            out.push_str(&self.sequence()?);
        }
        Ok(out)
    }

    fn sequence(&mut self) -> Result<String, SyntaxError> {
        let mut pieces: Vec<Piece> = Vec::new();
        loop {
            self.skip_comments();
            let Some(c) = self.peek() else { break };
            let atom = |text: String| Piece {
                text,
                state: State::Atom,
                zero_width: false,
            };
            let anchor = |text: String| Piece {
                text,
                state: State::Atom,
                zero_width: true,
            };
            match c {
                '|' | ')' => break,
                '(' => pieces.push(self.group()?),
                '[' => {
                    self.pos += 1;
                    let set = self.class()?;
                    let text = emit_set(&set);
                    pieces.push(if text == NEVER {
                        anchor(text)
                    } else {
                        atom(text)
                    });
                }
                '\\' => match self.escape(false)? {
                    Escape::Char(c) => pieces.push(atom(self.literal(c))),
                    Escape::Set(s) => {
                        let text = emit_set(&s);
                        pieces.push(if text == NEVER {
                            anchor(text)
                        } else {
                            atom(text)
                        });
                    }
                    Escape::Node(n) => pieces.push(atom(n)),
                    Escape::Anchor(n) => pieces.push(anchor(n)),
                    Escape::Quote => {
                        for c in self.quoted() {
                            pieces.push(atom(self.literal(c as u32)));
                        }
                    }
                },
                '*' | '+' | '?' => {
                    let at = self.here();
                    self.pos += 1;
                    match pieces.last_mut() {
                        Some(p) if p.state == State::Atom => {
                            let op = c.to_string();
                            self.quantify(p, &op, if c == '+' { 1 } else { 0 }, State::Starred);
                        }
                        _ => return Err(self.err(format!("Dangling meta character '{c}'"), at)),
                    }
                }
                '{' => {
                    let (op, min) = self.counted()?;
                    match pieces.last_mut() {
                        Some(p) if p.state == State::Atom => {
                            self.quantify(p, &op, min, State::Counted)
                        }
                        // After a quantifier, an inline flag group or nothing, `{n}` repeats the empty string.
                        _ => {
                            let mut empty = atom(String::new());
                            self.quantify(&mut empty, &op, min, State::Counted);
                            pieces.push(empty);
                        }
                    }
                }
                '^' => {
                    self.pos += 1;
                    pieces.push(anchor(self.caret()));
                }
                '$' => {
                    self.pos += 1;
                    pieces.push(anchor(self.dollar(flag(self.flags, MULTILINE))));
                }
                '.' => {
                    self.pos += 1;
                    pieces.push(atom(self.dot()));
                }
                _ => {
                    self.pos += 1;
                    pieces.push(atom(self.literal(c as u32)));
                }
            }
        }
        Ok(pieces.into_iter().map(|p| p.text).collect())
    }

    /// Applies quantifier `op` (at least `min` times) and its lazy (`?`) or possessive (`+`) modifier to `p`.
    fn quantify(&mut self, p: &mut Piece, op: &str, min: u32, state: State) {
        self.skip_comments();
        if p.zero_width {
            // An assertion repeated: optional when it may occur zero times, else the assertion itself.
            if min == 0 {
                p.text.clear();
            }
        }
        if p.text.is_empty() || p.zero_width {
            // Repeating nothing matches the empty string; fancy-regex rejects an empty repeat target.
            if matches!(self.peek(), Some('?' | '+')) {
                self.pos += 1;
            }
            p.state = state;
            return;
        }
        let body = format!("(?:{}){op}", p.text);
        p.text = match self.peek() {
            Some('?') => {
                self.pos += 1;
                format!("{body}?")
            }
            Some('+') => {
                self.pos += 1;
                format!("(?>{body})")
            }
            _ => body,
        };
        p.state = state;
    }

    /// `{n}`, `{n,}`, `{n,m}` after the `{`; returns the fancy-regex quantifier and its minimum.
    fn counted(&mut self) -> Result<(String, u32), SyntaxError> {
        self.pos += 1;
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.err("Illegal repetition", self.here()));
        }
        let min = self.number()?;
        self.skip_comments();
        let max = if self.peek() == Some(',') {
            self.pos += 1;
            self.skip_comments();
            let max = if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                Some(self.number()?)
            } else {
                None
            };
            self.skip_comments();
            max
        } else {
            Some(min)
        };
        if self.peek() != Some('}') {
            return Err(self.err("Unclosed counted closure", self.here()));
        }
        if max.is_some_and(|m| m < min) {
            return Err(self.err("Illegal repetition range", self.here()));
        }
        self.pos += 1;
        let op = match max {
            Some(m) if m == min => format!("{{{min}}}"),
            Some(m) => format!("{{{min},{m}}}"),
            None => format!("{{{min},}}"),
        };
        Ok((op, min))
    }

    fn number(&mut self) -> Result<u32, SyntaxError> {
        let mut n: u64 = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            n = n * 10 + d as u64;
            if n > i32::MAX as u64 {
                return Err(self.err("Illegal repetition range", self.here()));
            }
            self.pos += 1;
        }
        Ok(n as u32)
    }

    fn literal(&self, c: u32) -> String {
        emit_set(&literal_set(c, self.flags))
    }

    /// The text after `\Q` up to `\E` or the end of the pattern.
    fn quoted(&mut self) -> Vec<char> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            if c == '\\' && self.peek_at(1) == Some('E') {
                self.pos += 2;
                break;
            }
            out.push(c);
            self.pos += 1;
        }
        out
    }

    fn terminators(&self) -> &'static str {
        if flag(self.flags, UNIX_LINES) {
            "\n"
        } else {
            LINE_TERMINATORS
        }
    }

    /// Not between the `\r` and `\n` of a `\r\n` (only where `\r` is a line terminator).
    fn not_inside_crlf(&self) -> &'static str {
        if flag(self.flags, UNIX_LINES) {
            ""
        } else {
            "(?!(?<=\\x{D})\\x{A})"
        }
    }

    fn caret(&self) -> String {
        if !flag(self.flags, MULTILINE) {
            return "\\A".into();
        }
        // At the start or after a line terminator, but never at the end of the input.
        let after = emit_set(&Set::chars(self.terminators()));
        format!("(?:\\A|(?<={after}){})(?!\\z)", self.not_inside_crlf())
    }

    fn dollar(&self, multiline: bool) -> String {
        let terms = emit_set(&Set::chars(self.terminators()));
        let crlf = self.not_inside_crlf();
        match (multiline, flag(self.flags, UNIX_LINES)) {
            // Before any line terminator, or at the end.
            (true, _) => format!("(?:\\z|(?={terms}){crlf})"),
            // At the end, or before a line terminator that ends the input.
            (false, true) => "(?=\\x{A}?\\z)".into(),
            (false, false) => format!("(?=(?:\\x{{D}}\\x{{A}}|{terms})?\\z){crlf}"),
        }
    }

    fn dot(&self) -> String {
        if flag(self.flags, DOTALL) {
            "(?s:.)".into()
        } else {
            emit_set(&Set::chars(self.terminators()).negate())
        }
    }

    fn word(&self) -> Set {
        if flag(self.flags, UNICODE_CHARACTER_CLASS) {
            unicode_word()
        } else {
            Set::chars(ASCII_WORD)
        }
    }

    /// `\b` (or `\B`): a non-spacing mark counts as a word character when it follows one.
    fn boundary(&self, negated: bool) -> String {
        let w = self.word();
        let marks = gc("Mn");
        let (w_text, wm_text, mn_text) =
            (emit_set(&w), emit_set(&w.union(&marks)), emit_set(&marks));
        let left = format!("(?<={w_text}{mn_text}*)");
        let not_left = format!("(?<!{w_text}{mn_text}*)");
        if negated {
            format!("(?:{left}(?={wm_text})|{not_left}(?!{w_text}))")
        } else {
            format!("(?:{left}(?!{wm_text})|{not_left}(?={w_text}))")
        }
    }

    fn marks() -> Set {
        gc("Mn").union(&gc("Mc")).union(&gc("Me"))
    }

    fn group(&mut self) -> Result<Piece, SyntaxError> {
        self.pos += 1;
        let atom = |text: String| Piece {
            text,
            state: State::Atom,
            zero_width: false,
        };
        if self.peek() != Some('?') {
            self.groups += 1;
            let inner = self.group_body()?;
            return Ok(atom(format!("({inner})")));
        }
        self.pos += 1;
        let kind = self.peek();
        let wrap = |p: &mut Parser, open: &str| -> Result<Piece, SyntaxError> {
            let inner = p.group_body()?;
            let zero_width = matches!(open, "(?=" | "(?!" | "(?<=" | "(?<!")
                || (open == "(?:" && inner.is_empty());
            Ok(Piece {
                text: format!("{open}{inner})"),
                state: State::Atom,
                zero_width,
            })
        };
        match kind {
            Some(':') => {
                self.pos += 1;
                wrap(self, "(?:")
            }
            Some('=') => {
                self.pos += 1;
                wrap(self, "(?=")
            }
            Some('!') => {
                self.pos += 1;
                wrap(self, "(?!")
            }
            Some('>') => {
                self.pos += 1;
                wrap(self, "(?>")
            }
            Some('<') if matches!(self.peek_at(1), Some('=') | Some('!')) => {
                let open = if self.peek_at(1) == Some('=') {
                    "(?<="
                } else {
                    "(?<!"
                };
                self.pos += 2;
                wrap(self, open)
            }
            Some('<') => {
                self.pos += 1;
                let name = self.group_name()?;
                if self.names.contains_key(&name) {
                    return Err(self.err(
                        format!("Named capturing group <{name}> is already defined"),
                        self.here(),
                    ));
                }
                self.pos += 1;
                self.groups += 1;
                self.names.insert(name.clone(), self.groups);
                wrap(self, &format!("(?<{name}>"))
            }
            Some('$') | Some('@') => Err(self.err("Unknown group type", self.here())),
            _ => self.flag_group(),
        }
    }

    /// The group's contents and its `)`; inline flags set inside it end with it.
    fn group_body(&mut self) -> Result<String, SyntaxError> {
        let saved = self.flags;
        let inner = self.alternation()?;
        self.flags = saved;
        if self.peek() != Some(')') {
            return Err(self.err("Unclosed group", self.len()));
        }
        self.pos += 1;
        Ok(inner)
    }

    /// `(?idmsuxUc-idmsuxUc)` or `(?idmsuxUc-idmsuxUc:X)`, after the `?`.
    fn flag_group(&mut self) -> Result<Piece, SyntaxError> {
        let mut on = true;
        let (mut set, mut clear) = (0u32, 0u32);
        loop {
            let bit = match self.peek() {
                Some('i') => CASE_INSENSITIVE,
                Some('d') => UNIX_LINES,
                Some('m') => MULTILINE,
                Some('s') => DOTALL,
                Some('u') => UNICODE_CASE,
                Some('x') => COMMENTS,
                Some('U') => UNICODE_CHARACTER_CLASS,
                Some('c') => CANON_EQ,
                Some('-') if on => {
                    on = false;
                    self.pos += 1;
                    continue;
                }
                Some(')') => {
                    self.pos += 1;
                    self.flags = (self.flags | set) & !clear;
                    return Ok(Piece {
                        text: String::new(),
                        state: State::Flags,
                        zero_width: false,
                    });
                }
                Some(':') => {
                    self.pos += 1;
                    let saved = self.flags;
                    self.flags = (self.flags | set) & !clear;
                    let inner = self.group_body()?;
                    self.flags = saved;
                    return Ok(Piece {
                        zero_width: inner.is_empty(),
                        text: format!("(?:{inner})"),
                        state: State::Atom,
                    });
                }
                _ => return Err(self.err("Unknown inline modifier", self.here())),
            };
            if on {
                set |= bit;
            } else {
                clear |= bit;
            }
            self.pos += 1;
        }
    }

    /// A group name (ASCII letter, then ASCII letters and digits) up to and not including its `>`.
    fn group_name(&mut self) -> Result<String, SyntaxError> {
        if !self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            return Err(self.err(
                "capturing group name does not start with a Latin letter",
                self.here(),
            ));
        }
        let mut name = String::new();
        while let Some(c) = self.peek().filter(|c| c.is_ascii_alphanumeric()) {
            name.push(c);
            self.pos += 1;
        }
        if self.peek() != Some('>') {
            return Err(self.err("named capturing group is missing trailing '>'", self.here()));
        }
        Ok(name)
    }

    /// An escape sequence, at its backslash.
    fn escape(&mut self, in_class: bool) -> Result<Escape, SyntaxError> {
        self.pos += 1;
        let Some(c) = self.peek() else {
            let desc = if in_class {
                "Unclosed character class"
            } else {
                "Unescaped trailing backslash"
            };
            return Err(self.err(desc, self.len()));
        };
        let at = self.here();
        self.pos += 1;
        let unsupported = |p: &Parser| -> Result<Escape, SyntaxError> {
            Err(p.err("Illegal/unsupported escape sequence", at))
        };
        let unicode = flag(self.flags, UNICODE_CHARACTER_CLASS);
        Ok(match c {
            '0' => Escape::Char(self.octal()?),
            '1'..='9' if in_class => return unsupported(self),
            '1'..='9' => Escape::Node(self.backref(c)),
            'a' => Escape::Char(0x07),
            'e' => Escape::Char(0x1B),
            'f' => Escape::Char(0x0C),
            'n' => Escape::Char(0x0A),
            'r' => Escape::Char(0x0D),
            't' => Escape::Char(0x09),
            'c' => match self.peek() {
                Some(x) => {
                    self.pos += 1;
                    Escape::Char(x as u32 ^ 64)
                }
                None => return Err(self.err("Illegal control escape sequence", at)),
            },
            'x' => Escape::Char(self.hex()?),
            'u' => Escape::Char(self.unicode_escape()?),
            'N' => Escape::Char(self.char_name()?),
            'd' | 'D' => {
                let s = if unicode {
                    gc("Nd")
                } else {
                    ascii_range('0', '9')
                };
                Escape::Set(if c == 'D' { s.negate() } else { s })
            }
            'w' | 'W' => {
                let s = self.word();
                Escape::Set(if c == 'W' { s.negate() } else { s })
            }
            's' | 'S' => {
                let s = if unicode {
                    prop("IsWhite_Space")
                } else {
                    Set::chars(" \t\n\x0B\x0C\r")
                };
                Escape::Set(if c == 'S' { s.negate() } else { s })
            }
            'h' | 'H' => {
                let s = Set::chars(" \t\u{A0}\u{1680}\u{180E}\u{202F}\u{205F}\u{3000}")
                    .union(&Set(vec![(0x2000, 0x200A)]));
                Escape::Set(if c == 'H' { s.negate() } else { s })
            }
            'v' | 'V' => {
                let s = Set::chars("\n\x0B\x0C\r\u{85}\u{2028}\u{2029}");
                Escape::Set(if c == 'V' { s.negate() } else { s })
            }
            'p' | 'P' => Escape::Set(self.property(c == 'P')?),
            'Q' => Escape::Quote,
            _ if in_class
                && matches!(c, 'b' | 'B' | 'A' | 'G' | 'Z' | 'z' | 'R' | 'X' | 'k' | 'E') =>
            {
                return unsupported(self);
            }
            'b' if self.peek() == Some('{') && self.peek_at(1) == Some('g') => {
                if self.peek_at(2) != Some('}') {
                    return Err(self.err("Illegal/unsupported escape sequence", self.here() + 2));
                }
                self.pos += 3;
                // Approximation: no break inside \r\n or before a combining mark.
                let m = emit_set(&Parser::marks());
                Escape::Anchor(format!("(?:\\A|\\z|(?!(?<=\\x{{D}})\\x{{A}})(?!{m}))"))
            }
            'b' => Escape::Anchor(self.boundary(false)),
            'B' => Escape::Anchor(self.boundary(true)),
            'A' => Escape::Anchor("\\A".into()),
            'G' => Escape::Anchor("\\G".into()),
            'z' => Escape::Anchor("\\z".into()),
            'Z' => Escape::Anchor(self.dollar(false)),
            'R' => Escape::Node("(?:\\x{D}\\x{A}|[\\x{A}-\\x{D}\\x{85}\\x{2028}\\x{2029}])".into()),
            'X' => {
                // Approximation of an extended grapheme cluster: \r\n, or a character and its combining marks.
                let m = emit_set(&Parser::marks());
                Escape::Node(format!("(?>\\x{{D}}\\x{{A}}|(?s:.){m}*)"))
            }
            'k' => Escape::Node(self.named_backref()?),
            c if c.is_ascii_alphabetic() => return unsupported(self),
            c => Escape::Char(c as u32),
        })
    }

    /// `\0n`, `\0nn`, `\0mnn` (m <= 3), after the `0`.
    fn octal(&mut self) -> Result<u32, SyntaxError> {
        let digit = |c: Option<char>| c.and_then(|c| c.to_digit(8));
        let Some(first) = digit(self.peek()) else {
            return Err(self.err("Illegal octal escape sequence", self.here()));
        };
        self.pos += 1;
        let mut v = first;
        if let Some(d) = digit(self.peek()) {
            v = v * 8 + d;
            self.pos += 1;
            if first <= 3 {
                if let Some(d) = digit(self.peek()) {
                    v = v * 8 + d;
                    self.pos += 1;
                }
            }
        }
        Ok(v)
    }

    /// `\xhh` or `\x{h...h}`, after the `x`.
    fn hex(&mut self) -> Result<u32, SyntaxError> {
        let hex = |c: Option<char>| c.and_then(|c| c.to_digit(16));
        if self.peek() == Some('{') {
            let brace = self.here();
            self.pos += 1;
            if hex(self.peek()).is_none() {
                return Err(self.err("Illegal hexadecimal escape sequence", brace));
            }
            let mut v: u32 = 0;
            while let Some(d) = hex(self.peek()) {
                v = v * 16 + d;
                if v > MAX {
                    return Err(self.err("Hexadecimal codepoint is too big", self.here()));
                }
                self.pos += 1;
            }
            if self.peek() != Some('}') {
                return Err(self.err("Unclosed hexadecimal escape sequence", self.here()));
            }
            self.pos += 1;
            return Ok(v);
        }
        let mut v = 0;
        for _ in 0..2 {
            let Some(d) = hex(self.peek()) else {
                return Err(self.err("Illegal hexadecimal escape sequence", self.here()));
            };
            v = v * 16 + d;
            self.pos += 1;
        }
        Ok(v)
    }

    fn four_hex(&mut self) -> Result<u32, SyntaxError> {
        let mut v = 0;
        for _ in 0..4 {
            let Some(d) = self.peek().and_then(|c| c.to_digit(16)) else {
                return Err(self.err("Illegal Unicode escape sequence", self.here()));
            };
            v = v * 16 + d;
            self.pos += 1;
        }
        Ok(v)
    }

    /// `\uhhhh` after the `u`; an escaped surrogate pair `\uD83D\uDE00` is one code point.
    fn unicode_escape(&mut self) -> Result<u32, SyntaxError> {
        let high = self.four_hex()?;
        if (0xD800..0xDC00).contains(&high)
            && self.peek() == Some('\\')
            && self.peek_at(1) == Some('u')
        {
            let back = self.pos;
            self.pos += 2;
            match self.four_hex() {
                Ok(low) if (0xDC00..0xE000).contains(&low) => {
                    return Ok(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00));
                }
                _ => self.pos = back,
            }
        }
        Ok(high)
    }

    /// `\N{name}`: character names are not available here, so every name is reported unknown.
    fn char_name(&mut self) -> Result<u32, SyntaxError> {
        if self.peek() != Some('{') {
            return Err(self.err("Illegal character name escape sequence", self.here()));
        }
        let open = self.pos;
        let Some(close) = self.chars[open..]
            .iter()
            .position(|&c| c == '}')
            .map(|i| open + i)
        else {
            return Err(self.err("Unclosed character name escape sequence", self.len()));
        };
        let name: String = self.chars[open + 1..close].iter().collect();
        self.pos = close + 1;
        Err(self.err(format!("Unknown character name [{name}]"), close as isize))
    }

    /// `\n...` outside a class, after its first digit: further digits are taken while they name an existing group.
    fn backref(&mut self, first: char) -> String {
        let mut n = first.to_digit(10).unwrap_or(0) as usize;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            let longer = n * 10 + d as usize;
            if longer > self.groups {
                break;
            }
            n = longer;
            self.pos += 1;
        }
        let ci = if flag(self.flags, CASE_INSENSITIVE) {
            1
        } else {
            0
        };
        format!("{BACKREF}{n}{BACKREF}{ci}")
    }

    /// `\k<name>`, after the `k`.
    fn named_backref(&mut self) -> Result<String, SyntaxError> {
        if self.peek() != Some('<') {
            return Err(self.err(
                "\\k is not followed by '<' for named capturing group",
                self.here(),
            ));
        }
        self.pos += 1;
        let name = self.group_name()?;
        let Some(&n) = self.names.get(&name) else {
            return Err(self.err(
                format!("named capturing group <{name}> does not exist"),
                self.here(),
            ));
        };
        self.pos += 1;
        let ci = if flag(self.flags, CASE_INSENSITIVE) {
            1
        } else {
            0
        };
        Ok(format!("{BACKREF}{n}{BACKREF}{ci}"))
    }

    /// `\p{name}`, `\pX` (or `\P`), after the `p`.
    fn property(&mut self, negated: bool) -> Result<Set, SyntaxError> {
        let (name, at) = if self.peek() == Some('{') {
            let open = self.pos;
            let Some(close) = self.chars[open..]
                .iter()
                .position(|&c| c == '}')
                .map(|i| open + i)
            else {
                return Err(self.err("Unclosed character family", self.len()));
            };
            self.pos = close + 1;
            if close == open + 1 {
                return Err(self.err("Empty character family", close as isize));
            }
            (
                self.chars[open + 1..close].iter().collect::<String>(),
                close as isize,
            )
        } else {
            let at = self.here();
            match self.peek() {
                Some(c) => {
                    self.pos += 1;
                    (c.to_string(), at)
                }
                None => ("\0".to_string(), at),
            }
        };
        let (set, rule) = self.resolve_property(&name, at)?;
        let set = apply_case_rule(set, rule, self.flags);
        Ok(if negated { set.negate() } else { set })
    }

    fn resolve_property(&self, name: &str, at: isize) -> Result<(Set, CaseRule), SyntaxError> {
        let unicode = flag(self.flags, UNICODE_CHARACTER_CLASS);
        if let Some((key, value)) = name.split_once('=') {
            let found = match key.to_ascii_lowercase().as_str() {
                "sc" | "script" => script(value).map(|s| (s, CaseRule::Fixed)),
                "gc" | "general_category" => {
                    category(value).map(|s| (s, case_rule_of_category(value)))
                }
                "blk" | "block" => block(value).map(|s| (s, CaseRule::Fixed)),
                _ => None,
            };
            return found.ok_or_else(|| {
                self.err(
                    format!("Unknown Unicode property {{name=<{key}>, value=<{value}>}}"),
                    at,
                )
            });
        }
        let unknown = || self.err(format!("Unknown character property name {{{name}}}"), at);
        if let Some(rest) = name.strip_prefix("In") {
            return block(rest)
                .map(|s| (s, CaseRule::Fixed))
                .ok_or_else(unknown);
        }
        if let Some(rest) = name.strip_prefix("Is") {
            let upper = rest.to_ascii_uppercase();
            if let Some(s) = binary(&upper) {
                let rule = if matches!(
                    upper.as_str(),
                    "LOWERCASE" | "LOWER" | "UPPERCASE" | "UPPER" | "TITLECASE"
                ) {
                    CaseRule::CaseProperty
                } else {
                    CaseRule::Fixed
                };
                return Ok((s, rule));
            }
            if let Some(s) = category(rest) {
                return Ok((s, case_rule_of_category(rest)));
            }
            if let Some(s) = posix(rest, unicode) {
                return Ok((s, CaseRule::Fixed));
            }
            return script(rest)
                .map(|s| (s, CaseRule::Fixed))
                .ok_or_else(unknown);
        }
        if let Some(s) = category(name) {
            return Ok((s, case_rule_of_category(name)));
        }
        if let Some(s) = posix(name, unicode) {
            let rule = match name {
                "Lower" | "Upper" if unicode => CaseRule::CaseProperty,
                "Lower" | "Upper" => CaseRule::AsciiCase,
                _ => CaseRule::Fixed,
            };
            return Ok((s, rule));
        }
        if let Some(s) = java_class(name) {
            let rule = if matches!(name, "javaLowerCase" | "javaUpperCase" | "javaTitleCase") {
                CaseRule::CaseProperty
            } else {
                CaseRule::Fixed
            };
            return Ok((s, rule));
        }
        Err(unknown())
    }

    /// A character class after its `[`: items and nested classes are united, `&&` intersects what is on either side
    /// of it, and a leading `^` negates the whole class.
    fn class(&mut self) -> Result<Set, SyntaxError> {
        let negated = self.peek() == Some('^');
        if negated {
            self.pos += 1;
        }
        let mut result: Option<Set> = None;
        let mut current: Option<Set> = None;
        let mut first = true;
        loop {
            self.skip_comments();
            let Some(c) = self.peek() else {
                return Err(self.err("Unclosed character class", self.len() - 1));
            };
            match c {
                ']' if !first => {
                    self.pos += 1;
                    break;
                }
                '[' => {
                    self.pos += 1;
                    let nested = self.class()?;
                    current = Some(current.map_or(nested.clone(), |s| s.union(&nested)));
                }
                '&' if self.peek_at(1) == Some('&') => {
                    self.pos += 2;
                    if self.peek() == Some('&') && self.peek_at(1) != Some('&') {
                        // `&&&x`: JDK 21 unites the `&x` that follows instead of intersecting with it.
                        if current.is_none() && result.is_none() {
                            return Err(self.err("Bad class syntax", self.here() - 1));
                        }
                        first = false;
                        continue;
                    }
                    if let Some(s) = current.take() {
                        result = Some(result.map_or(s.clone(), |r| r.intersect(&s)));
                    }
                }
                _ => {
                    let item = self.class_item()?;
                    current = Some(current.map_or(item.clone(), |s| s.union(&item)));
                }
            }
            first = false;
        }
        if let Some(s) = current {
            result = Some(result.map_or(s.clone(), |r| r.intersect(&s)));
        }
        let Some(set) = result else {
            return Err(self.err("Bad class syntax", self.here() - 2));
        };
        Ok(if negated { set.negate() } else { set })
    }

    /// One class member: a character, a range, a predefined class or quoted text.
    fn class_item(&mut self) -> Result<Set, SyntaxError> {
        let lo = if self.peek() == Some('\\') {
            match self.escape(true)? {
                Escape::Char(c) => c,
                Escape::Set(s) => {
                    // A '-' after a predefined class is a literal '-'.
                    if self.peek() == Some('-') && self.peek_at(1) != Some('[') {
                        self.pos += 1;
                        return Ok(s.union(&class_fold(&Set::of('-' as u32), self.flags)));
                    }
                    return Ok(s);
                }
                Escape::Quote => {
                    let text = self.quoted();
                    return Ok(class_fold(
                        &Set::from(text.iter().map(|&c| (c as u32, c as u32)).collect()),
                        self.flags,
                    ));
                }
                Escape::Node(_) | Escape::Anchor(_) => {
                    unreachable!("anchors and back references are rejected inside a class")
                }
            }
        } else {
            let c = self.peek().map_or(0, |c| c as u32);
            self.pos += 1;
            c
        };
        if self.peek() == Some('-') {
            match self.peek_at(1) {
                None => return Err(self.err("Illegal character range", self.here() + 1)),
                Some(']') | Some('[') => {}
                Some('&') if self.peek_at(2) == Some('&') => {
                    return Err(self.err("Illegal character range", self.here() + 1));
                }
                Some(_) => {
                    self.pos += 1;
                    let hi = if self.peek() == Some('\\') {
                        if matches!(self.peek_at(1), Some('p' | 'P' | 'k' | '1'..='9')) {
                            return Err(
                                self.err("Illegal/unsupported escape sequence", self.here() + 1)
                            );
                        }
                        match self.escape(true)? {
                            Escape::Char(c) => c,
                            _ => return Err(self.err("Illegal character range", self.here() - 1)),
                        }
                    } else {
                        let c = self.peek().map_or(0, |c| c as u32);
                        self.pos += 1;
                        c
                    };
                    if hi < lo {
                        return Err(self.err("Illegal character range", self.here() - 1));
                    }
                    return Ok(range_fold(lo, hi, self.flags));
                }
            }
        }
        Ok(class_fold(&Set::of(lo), self.flags))
    }
}
