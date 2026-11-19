//! `java.text.SimpleDateFormat` and the `java.text.DateFormat` factories, written from the Java SE API
//! documentation, the recorded fixtures (`tests/fixtures/*/date_*.json`, `locale_dates.json`) and observations of a
//! JDK 21 (see `docs/provenance.md`).
//!
//! A pattern is read once into `Piece`s: literal text and pattern letters with their repeat count. Formatting
//! looks each letter's value up in a `GregorianCalendar` set to the instant; parsing walks the same pieces over the
//! text (`Scan`), collects the calendar fields it read in reading order and lets the calendar turn them into an
//! instant (lenient or not).

use super::decimal::{ParsePosition, unicode_digit};
use super::locale::{Lang, Locale};
use super::symbols::{
    DateFormatSymbols, DecimalFormatSymbols, date_pattern, date_time_pattern, time_pattern,
};
pub use super::symbols::{FULL, LONG, MEDIUM, SHORT};
use crate::lang::{JError, JValue};
use crate::util::GregorianCalendar;
use crate::util::TimeZone;
use crate::util::calendar::{
    AM_PM, DAY_OF_MONTH, DAY_OF_WEEK, DAY_OF_WEEK_IN_MONTH, DAY_OF_YEAR, DST_OFFSET, ERA, HOUR,
    HOUR_OF_DAY, MILLISECOND, MINUTE, MONTH, SECOND, WEEK_OF_MONTH, WEEK_OF_YEAR, YEAR,
    ZONE_OFFSET,
};
use crate::util::timezone;

/// Pseudo fields a parse records beside the calendar's: a week year (`Y`) and a `u` day number as read.
const WEEK_YEAR: usize = 100;
const ISO_DAY: usize = 101;

/// The pattern letters `SimpleDateFormat` documents.
const LETTERS: &str = "GyYMLwWDdFEuaHkKhmsSzZX";

/// The JDK release whose parsing rules apply where they differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Jdk {
    /// JDK 21 (Keycloak 26's runtime): a pattern literal must match the text's character exactly, also in
    /// lenient parsing.
    V21,
    /// JDK 26 (JDK 23 and later): lenient parsing lets any `SPACE_SEPARATOR` match another (JDK-8324665), so
    /// an ASCII space matches the U+202F CLDR puts before AM/PM.
    #[default]
    V26,
}

/// One element of a compiled pattern.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    /// Text copied to the output and matched in the input.
    Text(String),
    /// A pattern letter repeated `count` times.
    Letter(char, usize),
}

impl Piece {
    /// Whether the piece formats as a number (and so, followed by another such piece, parses a fixed width).
    fn is_numeric(&self) -> bool {
        match *self {
            Piece::Letter('M' | 'L', n) => n <= 2,
            Piece::Letter(c, _) => "yYwWDdFuHkKhmsS".contains(c),
            Piece::Text(_) => false,
        }
    }
}

fn illegal_argument(message: impl Into<String>) -> JError {
    JError::new("java.lang.IllegalArgumentException", message)
}

/// Reads a pattern: `'` opens and closes quoted text, `''` is a quote, ASCII letters are pattern letters (only those
/// in [`LETTERS`]), everything else is literal text.
fn read_pattern(pattern: &str) -> Result<Vec<Piece>, JError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String, pieces: &mut Vec<Piece>| {
        if !text.is_empty() {
            pieces.push(Piece::Text(std::mem::take(text)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                text.push('\'');
                i += 2;
                continue;
            }
            // Quoted text up to the closing quote; a doubled quote inside stands for one.
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(illegal_argument("Unterminated quote")),
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        text.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(&q) => {
                        text.push(q);
                        i += 1;
                    }
                }
            }
            continue;
        }
        if c.is_ascii_alphabetic() {
            if !LETTERS.contains(c) {
                return Err(illegal_argument(format!("Illegal pattern character '{c}'")));
            }
            let run = chars[i..].iter().take_while(|&&d| d == c).count();
            if c == 'X' && run > 3 {
                return Err(illegal_argument(format!(
                    "invalid ISO 8601 format: length={run}"
                )));
            }
            flush(&mut text, &mut pieces);
            pieces.push(Piece::Letter(c, run));
            i += run;
            continue;
        }
        text.push(c);
        i += 1;
    }
    flush(&mut text, &mut pieces);
    Ok(pieces)
}

/// `java.text.SimpleDateFormat`.
#[derive(Clone)]
pub struct SimpleDateFormat {
    pattern: String,
    pieces: Vec<Piece>,
    locale: Locale,
    zone: TimeZone,
    symbols: DateFormatSymbols,
    zero_digit: char,
    /// What a negative number starts with (`DecimalFormatSymbols.getMinusSignText()`).
    minus: String,
    lenient: bool,
    jdk: Jdk,
    /// `get2DigitYearStart()` in milliseconds.
    century_start: i64,
    /// The year of `century_start` in the zone the formatter had when it was set.
    century_start_year: i32,
}

impl std::fmt::Debug for SimpleDateFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimpleDateFormat")
            .field("pattern", &self.pattern)
            .field("locale", &self.locale.to_string())
            .field("zone", &self.zone.id())
            .field("lenient", &self.lenient)
            .field("jdk", &self.jdk)
            .finish()
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl SimpleDateFormat {
    /// `new SimpleDateFormat(pattern, locale)` with `setTimeZone(zone)`: lenient, the two-digit-year window starting
    /// 80 years before now.
    pub fn new(
        pattern: &str,
        locale: &Locale,
        zone: &TimeZone,
    ) -> Result<SimpleDateFormat, JError> {
        let pieces = read_pattern(pattern)?;
        let numbers = DecimalFormatSymbols::new(locale);
        let mut f = SimpleDateFormat {
            pattern: pattern.to_string(),
            pieces,
            locale: locale.clone(),
            zone: zone.clone(),
            symbols: DateFormatSymbols::new(locale),
            zero_digit: numbers.zero_digit,
            minus: numbers.minus_text(),
            lenient: true,
            jdk: Jdk::default(),
            century_start: 0,
            century_start_year: 0,
        };
        let mut c = f.calendar(now_millis());
        let _ = c.add(YEAR, -80);
        f.set_2digit_year_start(c.time_in_millis());
        Ok(f)
    }

    fn calendar(&self, millis: i64) -> GregorianCalendar {
        GregorianCalendar::new(millis, &self.zone, &self.locale)
    }

    /// `set2DigitYearStart(date)`.
    pub fn set_2digit_year_start(&mut self, millis: i64) {
        self.century_start = millis;
        self.century_start_year = self.calendar(millis).get(YEAR);
    }

    /// `get2DigitYearStart()`.
    pub fn two_digit_year_start(&self) -> i64 {
        self.century_start
    }

    /// The locale it formats for.
    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    /// `getTimeZone()`: after parsing a zone name, that zone.
    pub fn time_zone(&self) -> &TimeZone {
        &self.zone
    }

    /// `setTimeZone(zone)`.
    pub fn set_time_zone(&mut self, zone: &TimeZone) {
        self.zone = zone.clone();
    }

    /// The JDK whose parsing rules apply.
    pub fn jdk(&self) -> Jdk {
        self.jdk
    }

    /// Which JDK's behaviour to follow where they differ (JDK 26 by default).
    pub fn set_jdk(&mut self, jdk: Jdk) {
        self.jdk = jdk;
    }

    /// [`SimpleDateFormat::set_jdk`], as a builder.
    pub fn with_jdk(mut self, jdk: Jdk) -> SimpleDateFormat {
        self.jdk = jdk;
        self
    }

    /// `isLenient()`.
    pub fn is_lenient(&self) -> bool {
        self.lenient
    }

    /// `setLenient(lenient)`.
    pub fn set_lenient(&mut self, lenient: bool) {
        self.lenient = lenient;
    }

    /// `toPattern()`.
    pub fn to_pattern(&self) -> String {
        self.pattern.clone()
    }

    /// `toLocalizedPattern()`. The JDK's localized pattern characters are the standard ones
    /// (`GyMdkHmsSEDFwWahKzZ`) in every locale this crate has data for, and letters beyond them stay as they are, so
    /// the localized pattern is the pattern.
    pub fn to_localized_pattern(&self) -> Result<String, JError> {
        Ok(self.pattern.clone())
    }

    /// `applyPattern(pattern)`.
    pub fn apply_pattern(&mut self, pattern: &str) -> Result<(), JError> {
        self.pieces = read_pattern(pattern)?;
        self.pattern = pattern.to_string();
        Ok(())
    }

    /// `format(new Date(millis))`.
    pub fn format(&mut self, millis: i64) -> String {
        let mut cal = self.calendar(millis);
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Letter(c, n) => self.format_letter(&mut cal, *c, *n, &mut out),
            }
        }
        out
    }

    /// `format(Object)`: a `Date` or a `Number` (its `longValue()` as milliseconds).
    pub fn format_value(&mut self, value: &JValue) -> Result<String, JError> {
        if let Some(millis) = super::objects::date_millis(value) {
            return Ok(self.format(millis));
        }
        match super::number_long_value(value) {
            Some(millis) => Ok(self.format(millis)),
            None => Err(illegal_argument("Cannot format given Object as a Date")),
        }
    }

    /// Whether `M` gives standalone month names: when it is the pattern's only field (the Javadoc's "standalone
    /// pattern", `MMMM` alone; literals around it do not count).
    fn standalone_month(&self) -> bool {
        let mut letters = self
            .pieces
            .iter()
            .filter(|p| matches!(p, Piece::Letter(..)));
        matches!(letters.next(), Some(Piece::Letter('M', _))) && letters.next().is_none()
    }

    /// A number with at least `width` digits in the locale's digits and minus sign.
    fn number(&self, value: i64, width: usize, out: &mut String) {
        if value < 0 {
            out.push_str(&self.minus);
        }
        let digits = value.unsigned_abs().to_string();
        let zero = self.zero_digit as u32;
        for _ in digits.len()..width {
            out.push(self.zero_digit);
        }
        for d in digits.bytes() {
            out.push(char::from_u32(zero + (d - b'0') as u32).unwrap_or(d as char));
        }
    }

    fn format_letter(&self, cal: &mut GregorianCalendar, letter: char, n: usize, out: &mut String) {
        let s = &self.symbols;
        match letter {
            'G' => out.push_str(s.eras[cal.get(ERA) as usize]),
            'y' | 'Y' => {
                let year = if letter == 'y' {
                    cal.get(YEAR)
                } else {
                    cal.week_year()
                } as i64;
                if n == 2 {
                    self.number(year % 100, 2, out);
                } else {
                    self.number(year, n, out);
                }
            }
            'M' | 'L' => {
                let m = cal.get(MONTH) as usize;
                let (long, short) = if letter == 'M' && !self.standalone_month() {
                    (&s.months, &s.short_months)
                } else {
                    (&s.standalone_months, &s.standalone_short_months)
                };
                match n {
                    1 | 2 => self.number(m as i64 + 1, n, out),
                    3 => out.push_str(short[m]),
                    _ => out.push_str(long[m]),
                }
            }
            'E' => {
                let d = cal.get(DAY_OF_WEEK) as usize - 1;
                out.push_str(if n >= 4 {
                    s.weekdays[d]
                } else {
                    s.short_weekdays[d]
                });
            }
            'u' => {
                let d = cal.get(DAY_OF_WEEK);
                self.number(if d == 1 { 7 } else { d as i64 - 1 }, n, out);
            }
            'a' => out.push_str(s.am_pm[cal.get(AM_PM) as usize]),
            'k' => {
                let h = cal.get(HOUR_OF_DAY);
                self.number(if h == 0 { 24 } else { h as i64 }, n, out);
            }
            'h' => {
                let h = cal.get(HOUR);
                self.number(if h == 0 { 12 } else { h as i64 }, n, out);
            }
            'z' => {
                let daylight = cal.get(DST_OFFSET) != 0;
                let style = if n >= 4 {
                    timezone::LONG
                } else {
                    timezone::SHORT
                };
                out.push_str(&self.zone.display_name(daylight, style, &self.locale));
            }
            'Z' => {
                let minutes = (cal.get(ZONE_OFFSET) + cal.get(DST_OFFSET)) / 60_000;
                out.push(if minutes < 0 { '-' } else { '+' });
                // RFC 822 offsets are always in ASCII digits.
                let m = minutes.abs();
                out.push_str(&format!("{:02}{:02}", m / 60, m % 60));
            }
            'X' => {
                let minutes = (cal.get(ZONE_OFFSET) + cal.get(DST_OFFSET)) / 60_000;
                if minutes == 0 {
                    out.push('Z');
                    return;
                }
                out.push(if minutes < 0 { '-' } else { '+' });
                let m = minutes.abs();
                out.push_str(&format!("{:02}", m / 60));
                match n {
                    1 => {}
                    2 => out.push_str(&format!("{:02}", m % 60)),
                    _ => out.push_str(&format!(":{:02}", m % 60)),
                }
            }
            _ => {
                let field = match letter {
                    'w' => WEEK_OF_YEAR,
                    'W' => WEEK_OF_MONTH,
                    'D' => DAY_OF_YEAR,
                    'd' => DAY_OF_MONTH,
                    'F' => DAY_OF_WEEK_IN_MONTH,
                    'H' => HOUR_OF_DAY,
                    'K' => HOUR,
                    'm' => MINUTE,
                    's' => SECOND,
                    _ => MILLISECOND,
                };
                self.number(cal.get(field) as i64, n, out);
            }
        }
    }

    /// `parse(text)` from the start: `None` when it fails.
    pub fn parse(&mut self, text: &str) -> Option<i64> {
        let mut pos = ParsePosition::new(0);
        self.parse_at(text, &mut pos)
    }

    /// `DateFormat.parse(String)`: a `ParseException` when the text does not start with a date.
    pub fn parse_checked(&mut self, text: &str) -> Result<i64, JError> {
        self.parse(text).ok_or_else(|| {
            JError::new(
                "java.text.ParseException",
                format!("Unparseable date: \"{text}\""),
            )
        })
    }

    /// `parse(text, pos)`: on success `pos.index` moves past the date; on failure it stays and `pos.error_index` is
    /// where the text stopped matching.
    pub fn parse_at(&mut self, text: &str, pos: &mut ParsePosition) -> Option<i64> {
        let chars: Vec<char> = text.chars().collect();
        let mut scan = Scan {
            f: self,
            text: &chars,
            at: pos.index,
            fields: Vec::new(),
            two_digit_year: false,
        };
        let outcome = scan.run();
        let Scan {
            at,
            fields,
            two_digit_year,
            ..
        } = scan;
        let result = outcome.and_then(|()| self.resolve(&fields, two_digit_year).ok_or(at));
        match result {
            Ok(millis) => {
                pos.index = at;
                Some(millis)
            }
            Err(error) => {
                pos.error_index = error as i64;
                None
            }
        }
    }

    /// The instant of the fields read, in reading order; `None` when a non-lenient calendar rejects them.
    ///
    /// A week year read after the last calendar year (or without one) makes the date a week date: that week year,
    /// the last week of year read (else 1) and the last day of week read (else the first day of the week); the other
    /// date fields are then ignored. A `u` day number outside 1..=7 is the plain calendar value otherwise, and in a
    /// week date counts on into the weeks around (0 is the Sunday a week before 7's); a strict parse rejects it there.
    fn resolve(&self, fields: &[(usize, i32)], two_digit_year: bool) -> Option<i64> {
        let last = |wanted: &[usize]| fields.iter().rposition(|f| wanted.contains(&f.0));
        let week_date = match (last(&[WEEK_YEAR]), last(&[YEAR])) {
            (Some(w), Some(y)) => w > y,
            (Some(_), None) => true,
            _ => false,
        };
        let build = |year_shift: i32| -> Option<i64> {
            let mut cal = self.calendar(0);
            cal.clear();
            cal.set_lenient(self.lenient);
            for &(field, value) in fields {
                match field {
                    WEEK_YEAR => {}
                    ISO_DAY => cal.set(
                        DAY_OF_WEEK,
                        if (1..=7).contains(&value) {
                            value % 7 + 1
                        } else {
                            value
                        },
                    ),
                    YEAR => cal.set(YEAR, value + year_shift),
                    _ => cal.set(field, value),
                }
            }
            if week_date {
                let week_year = fields[last(&[WEEK_YEAR])?].1 + year_shift;
                let week = last(&[WEEK_OF_YEAR]).map_or(1, |i| fields[i].1);
                let (day, weeks) = match last(&[DAY_OF_WEEK, ISO_DAY]).map(|i| fields[i]) {
                    Some((ISO_DAY, v)) => (v.rem_euclid(7) + 1, (v - 1).div_euclid(7)),
                    Some((_, v)) => (v, 0),
                    None => (cal.first_day_of_week(), 0),
                };
                if weeks != 0 && !self.lenient {
                    return None;
                }
                cal.set_week_date(week_year, week + weeks, day).ok()?;
            }
            cal.checked_time_in_millis().ok()
        };
        let millis = build(0)?;
        // A two-digit year before the window's start belongs to the next century (only where that moves it later:
        // not for a year before the common era).
        if two_digit_year && millis < self.century_start {
            if let Some(later) = build(100).filter(|later| *later > millis) {
                return Some(later);
            }
        }
        Some(millis)
    }
}

/// One parse of a text against a formatter's pattern.
struct Scan<'a> {
    f: &'a mut SimpleDateFormat,
    text: &'a [char],
    at: usize,
    /// Calendar fields read, in order.
    fields: Vec<(usize, i32)>,
    /// A two-digit year was read into the century window; if the result falls before the window it moves 100 years.
    two_digit_year: bool,
}

fn is_space_separator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// The single-character upper and lower case of `c` (itself where the mapping is not one character).
fn case_forms(c: char) -> (char, char) {
    let single = |mut it: std::iter::Peekable<std::vec::IntoIter<char>>| {
        let first = it.next();
        if it.peek().is_none() { first } else { None }
    };
    let up = single(c.to_uppercase().collect::<Vec<_>>().into_iter().peekable()).unwrap_or(c);
    let low = single(c.to_lowercase().collect::<Vec<_>>().into_iter().peekable()).unwrap_or(c);
    (up, low)
}

/// Whether `a` and `b` are equal ignoring case, compared character by character (`String.regionMatches(true, ..)`).
fn same_ignoring_case(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    let (au, _) = case_forms(a);
    let (bu, _) = case_forms(b);
    if au == bu {
        return true;
    }
    case_forms(au).1 == case_forms(bu).1
}

impl Scan<'_> {
    fn run(&mut self) -> Result<(), usize> {
        let pieces = self.f.pieces.clone();
        for (k, piece) in pieces.iter().enumerate() {
            match piece {
                Piece::Text(t) => self.literal(t)?,
                Piece::Letter(c, n) => {
                    let fixed =
                        piece.is_numeric() && pieces.get(k + 1).is_some_and(Piece::is_numeric);
                    self.letter(*c, *n, fixed)?;
                }
            }
        }
        Ok(())
    }

    fn literal(&mut self, t: &str) -> Result<(), usize> {
        let loose = self.f.lenient && self.f.jdk == Jdk::V26;
        for p in t.chars() {
            match self.text.get(self.at) {
                Some(&c) if c == p || (loose && is_space_separator(p) && is_space_separator(c)) => {
                    self.at += 1
                }
                _ => return Err(self.at),
            }
        }
        Ok(())
    }

    /// Skips the spaces and tabs before a field.
    fn skip_blanks(&mut self) {
        while matches!(self.text.get(self.at), Some(' ' | '\t')) {
            self.at += 1;
        }
    }

    /// A decimal integer, optionally negative, of at most `max` characters: its value and where it ends.
    fn integer(&self, start: usize, max: Option<usize>) -> Option<(i64, usize)> {
        let end = max.map_or(self.text.len(), |m| (start + m).min(self.text.len()));
        let mut i = start;
        let negative = self.text.get(i) == Some(&'-') && i < end;
        if negative {
            i += 1;
        }
        let digits_start = i;
        let mut value: i64 = 0;
        while i < end {
            let Some(d) = unicode_digit(self.text[i]) else {
                break;
            };
            value = value.saturating_mul(10).saturating_add(d as i64);
            i += 1;
        }
        if i == digits_start {
            return None;
        }
        Some((if negative { -value } else { value }, i))
    }

    /// The longest of `names` the text continues with, ignoring case: its index and length.
    fn longest(&self, names: &[&str]) -> Option<(usize, usize)> {
        let mut best: Option<(usize, usize)> = None;
        for (i, name) in names.iter().enumerate() {
            let len = name.chars().count();
            if len == 0 || best.is_some_and(|b| b.1 >= len) {
                continue;
            }
            if self.at + len <= self.text.len()
                && name
                    .chars()
                    .zip(&self.text[self.at..])
                    .all(|(a, &b)| same_ignoring_case(a, b))
            {
                best = Some((i, len));
            }
        }
        best
    }

    /// The longest of `first` the text continues with, else the longest of `second`.
    fn text_field(&mut self, first: &[&str], second: &[&str]) -> Option<usize> {
        let (i, len) = self.longest(first).or_else(|| self.longest(second))?;
        self.at += len;
        Some(i)
    }

    fn letter(&mut self, letter: char, n: usize, fixed: bool) -> Result<(), usize> {
        let field_start = self.at;
        self.skip_blanks();
        if self.at >= self.text.len() {
            return Err(field_start);
        }
        let start = self.at;
        let s = self.f.symbols.clone();
        let is_text = matches!(letter, 'G' | 'E' | 'a') || (matches!(letter, 'M' | 'L') && n >= 3);
        if is_text && start != field_start {
            // Names are matched where the field begins: blanks before them are not skipped.
            return Err(start);
        }
        let standalone_long: Vec<&str> = s
            .standalone_months
            .iter()
            .chain(&s.months)
            .copied()
            .collect();
        let standalone_short: Vec<&str> = s
            .standalone_short_months
            .iter()
            .chain(&s.short_months)
            .copied()
            .collect();
        let text_value = match letter {
            'G' => Some((ERA, self.text_field(&s.eras, &[]))),
            'M' if n >= 3 && self.f.standalone_month() => Some((
                MONTH,
                self.text_field(&s.standalone_months, &s.standalone_short_months),
            )),
            'M' if n >= 3 => Some((MONTH, self.text_field(&s.months, &s.short_months))),
            // Standalone names, and the format names too.
            'L' if n >= 3 => Some((
                MONTH,
                self.text_field(&standalone_long, &standalone_short)
                    .map(|m| m % 12),
            )),
            'E' => Some((
                DAY_OF_WEEK,
                self.text_field(&s.weekdays, &s.short_weekdays)
                    .map(|d| d + 1),
            )),
            'a' => Some((AM_PM, self.text_field(&s.am_pm, &[]))),
            'z' | 'Z' => return self.general_zone(start),
            'X' => return self.iso_zone(n, start),
            _ => None,
        };
        if let Some((field, value)) = text_value {
            let value = value.ok_or(start)?;
            self.fields.push((field, value as i32));
            return Ok(());
        }
        let (value, end) = self.integer(start, fixed.then_some(n)).ok_or(start)?;
        self.at = end;
        let v = value as i32;
        let (field, v) = match letter {
            'y' | 'Y' => {
                let mut year = v;
                let digits = &self.text[start..end];
                if n <= 2 && digits.len() == 2 && digits.iter().all(|c| unicode_digit(*c).is_some())
                {
                    let window = self.f.century_start_year;
                    let century = window.div_euclid(100) * 100;
                    year = century + v;
                    if year < window {
                        year += 100;
                    }
                    self.two_digit_year = true;
                }
                (if letter == 'Y' { WEEK_YEAR } else { YEAR }, year)
            }
            'M' | 'L' => (MONTH, v - 1),
            // Monday (1) to Sunday (7); turned into a calendar day of week when the date is resolved.
            'u' => (ISO_DAY, v),
            'k' => {
                if !self.f.lenient && !(1..=24).contains(&v) {
                    return Err(end);
                }
                (HOUR_OF_DAY, if v == 24 { 0 } else { v })
            }
            'h' => {
                if !self.f.lenient && !(1..=12).contains(&v) {
                    return Err(end);
                }
                (HOUR, if v == 12 { 0 } else { v })
            }
            'w' => (WEEK_OF_YEAR, v),
            'W' => (WEEK_OF_MONTH, v),
            'D' => (DAY_OF_YEAR, v),
            'd' => (DAY_OF_MONTH, v),
            'F' => (DAY_OF_WEEK_IN_MONTH, v),
            'H' => (HOUR_OF_DAY, v),
            'K' => (HOUR, v),
            'm' => (MINUTE, v),
            's' => (SECOND, v),
            _ => (MILLISECOND, v),
        };
        self.fields.push((field, v));
        Ok(())
    }

    /// Records a zone offset read from the text (milliseconds, no daylight saving).
    fn offset(&mut self, millis: i32) {
        self.fields.push((ZONE_OFFSET, millis));
        self.fields.push((DST_OFFSET, 0));
    }

    /// Two ASCII digits at the current position, at most `max`; an error at the first missing digit, or at the
    /// second one for a value above `max`.
    fn two_digits(&mut self, max: i32) -> Result<i32, usize> {
        let mut v = 0;
        for _ in 0..2 {
            match self.text.get(self.at).and_then(|c| c.to_digit(10)) {
                Some(d) => v = v * 10 + d as i32,
                None => return Err(self.at),
            }
            self.at += 1;
        }
        if v > max {
            return Err(self.at - 1);
        }
        Ok(v)
    }

    fn sign(&self) -> Option<i32> {
        match self.text.get(self.at) {
            Some('+') => Some(1),
            Some('-') => Some(-1),
            _ => None,
        }
    }

    /// `z` and `Z`: `GMT` (any case) optionally followed by `±h[h]:mm`, an RFC 822 offset (`-0800`), or a zone name
    /// (which also makes that zone the formatter's).
    fn general_zone(&mut self, start: usize) -> Result<(), usize> {
        let gmt = self.text.len() >= self.at + 3
            && "GMT"
                .chars()
                .zip(&self.text[self.at..])
                .all(|(a, &b)| same_ignoring_case(a, b));
        if gmt {
            self.at += 3;
            let Some(sign) = self.sign() else {
                self.offset(0);
                return Ok(());
            };
            self.at += 1;
            let minutes = self.gmt_offset()?;
            self.offset(sign * minutes * 60_000);
            return Ok(());
        }
        if let Some(sign) = self.sign() {
            self.at += 1;
            let hours = self.two_digits(23)?;
            let minutes = self.two_digits(59)?;
            self.offset(sign * (hours * 60 + minutes) * 60_000);
            return Ok(());
        }
        self.zone_name().ok_or(start)
    }

    /// The `h[h]:mm` after `GMT±`, in minutes.
    fn gmt_offset(&mut self) -> Result<i32, usize> {
        let digit = |s: &Self| {
            s.text
                .get(s.at)
                .and_then(|c| c.to_digit(10))
                .map(|d| d as i32)
        };
        let mut hours = digit(self).ok_or(self.at)?;
        self.at += 1;
        if let Some(d) = digit(self) {
            hours = hours * 10 + d;
            if hours > 23 {
                return Err(self.at);
            }
            self.at += 1;
        }
        if self.text.get(self.at) != Some(&':') {
            return Err(self.at);
        }
        self.at += 1;
        let minutes = self.two_digits(59)?;
        Ok(hours * 60 + minutes)
    }

    /// A zone name: one of the formatter's own zone's, else the zone the JDK finds for it among the names of its
    /// locale data (`tz_parse_names.rs`).
    fn zone_name(&mut self) -> Option<()> {
        let locale = self.f.locale.clone();
        let own = &self.f.zone;
        let names = [
            own.display_name(false, timezone::LONG, &locale),
            own.display_name(false, timezone::SHORT, &locale),
            own.display_name(true, timezone::LONG, &locale),
            own.display_name(true, timezone::SHORT, &locale),
        ];
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        if let Some((i, len)) = self.longest(&refs) {
            self.at += len;
            let zone = self.f.zone.clone();
            self.named_zone(&zone, i >= 2);
            return Some(());
        }
        let language = match locale.lang() {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Root => "root",
        };
        let table = crate::util::tz_parse_names::ZONE_PARSE_NAMES;
        let rows: Vec<_> = table.iter().filter(|r| r.0 == language).collect();
        let keys: Vec<&str> = rows.iter().map(|r| r.1).collect();
        let (i, len) = self.longest(&keys)?;
        let (_, name, id, zone_names) = *rows[i];
        let zone = TimeZone::get(id);
        let daylight = !zone_names[..2].contains(&name) && zone_names[2..].contains(&name);
        self.at += len;
        self.named_zone(&zone, daylight);
        self.f.zone = zone;
        Some(())
    }

    /// Records a zone whose name was read: its daylight saving (none for a standard-time name); its standard offset
    /// is the zone's own at the date read, left to the calendar.
    fn named_zone(&mut self, zone: &TimeZone, daylight: bool) {
        let saving = match zone.dst_savings() {
            0 if daylight => 3_600_000,
            s if daylight => s,
            _ => 0,
        };
        self.fields.retain(|f| f.0 != ZONE_OFFSET);
        self.fields.push((DST_OFFSET, saving));
    }

    /// `X`: `Z`, or a sign with `hh` (`X`), `hhmm` (`XX`) or `hh:mm` (`XXX`); `X` also takes `hhmm`'s first two
    /// digits only.
    fn iso_zone(&mut self, n: usize, start: usize) -> Result<(), usize> {
        if self.text.get(self.at) == Some(&'Z') {
            self.at += 1;
            self.offset(0);
            return Ok(());
        }
        // Anything but `Z` or a sign is reported one character on.
        let sign = self.sign().ok_or(start + 1)?;
        self.at += 1;
        let hours = self.two_digits(23)?;
        let minutes = match n {
            1 => 0,
            2 => self.two_digits(59)?,
            _ => {
                if self.text.get(self.at) != Some(&':') {
                    return Err(self.at);
                }
                self.at += 1;
                self.two_digits(59)?
            }
        };
        self.offset(sign * (hours * 60 + minutes) * 60_000);
        Ok(())
    }
}

/// The `java.text.DateFormat` factories.
pub struct DateFormat;

fn check_style(style: i32, what: &str) -> Result<(), JError> {
    if (FULL..=SHORT).contains(&style) {
        Ok(())
    } else {
        Err(illegal_argument(format!("Illegal {what} style {style}")))
    }
}

impl DateFormat {
    /// A date, time or date-time instance; `None` for a style leaves that part out (both `None`: `getInstance()`,
    /// `SHORT` date and time).
    pub fn instance(
        date_style: Option<i32>,
        time_style: Option<i32>,
        locale: &Locale,
        zone: &TimeZone,
    ) -> Result<SimpleDateFormat, JError> {
        let pattern = match (date_style, time_style) {
            (Some(d), Some(t)) => {
                check_style(t, "time")?;
                check_style(d, "date")?;
                date_time_pattern(
                    d,
                    t,
                    date_pattern(d, locale),
                    time_pattern(t, locale),
                    locale,
                )
            }
            (Some(d), None) => {
                check_style(d, "date")?;
                date_pattern(d, locale).to_string()
            }
            (None, Some(t)) => {
                check_style(t, "time")?;
                time_pattern(t, locale).to_string()
            }
            (None, None) => date_time_pattern(
                SHORT,
                SHORT,
                date_pattern(SHORT, locale),
                time_pattern(SHORT, locale),
                locale,
            ),
        };
        SimpleDateFormat::new(&pattern, locale, zone)
    }

    /// `DateFormat.getDateInstance(style, locale)` in `zone`.
    pub fn date_instance(
        style: i32,
        locale: &Locale,
        zone: &TimeZone,
    ) -> Result<SimpleDateFormat, JError> {
        Self::instance(Some(style), None, locale, zone)
    }

    /// `DateFormat.getTimeInstance(style, locale)` in `zone`.
    pub fn time_instance(
        style: i32,
        locale: &Locale,
        zone: &TimeZone,
    ) -> Result<SimpleDateFormat, JError> {
        Self::instance(None, Some(style), locale, zone)
    }

    /// `DateFormat.getDateTimeInstance(dateStyle, timeStyle, locale)` in `zone`.
    pub fn date_time_instance(
        date_style: i32,
        time_style: i32,
        locale: &Locale,
        zone: &TimeZone,
    ) -> Result<SimpleDateFormat, JError> {
        Self::instance(Some(date_style), Some(time_style), locale, zone)
    }
}
