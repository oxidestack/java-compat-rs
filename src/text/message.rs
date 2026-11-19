//! `java.text.MessageFormat` and `java.text.ChoiceFormat`.
//!
//! Clean-room implementation: written from the Java SE API specification of both classes
//! (pattern grammar, the `format` argument table, `nextDouble`/`previousDouble`), the recorded fixtures in
//! `tests/fixtures` (JDK 26) and observations of an arm64 JDK 21, without OpenJDK source. `docs/provenance.md` lists what each corner rests on.
//!
//! The model: a [`ChoiceFormat`] is a list of `(limit, text)` choices; a [`MessageFormat`] is a list of `Piece`s,
//! literal text or an argument reference with an optional [`SubFormat`]. `toPattern` regenerates both from that
//! model, never from the source pattern.

use std::fmt;

use super::{DateFormat, DecimalFormat, Locale, NumberFormat, SimpleDateFormat};
use crate::lang::{JError, JValue};
use crate::util::TimeZone;

const ILLEGAL_ARGUMENT: &str = "java.lang.IllegalArgumentException";
const NUMBER_FORMAT: &str = "java.lang.NumberFormatException";

fn illegal(message: impl Into<String>) -> JError {
    JError::new(ILLEGAL_ARGUMENT, message)
}

/// `DateFormat` style constants.
const FULL: i32 = 0;
const LONG: i32 = 1;
const MEDIUM: i32 = 2;
const SHORT: i32 = 3;

/// Java's `String.trim()`: drops every char up to U+0020 from both ends.
fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

// -------------------------------------------------------------------------------------------------------------
// Double.parseDouble

/// `Double.parseDouble`: surrounding control chars and spaces are ignored; then an optional sign and `NaN`,
/// `Infinity`, a decimal literal or a hexadecimal literal with a binary exponent, each optionally followed by one
/// of `fFdD`.
pub(crate) fn java_parse_double(s: &str) -> Result<f64, JError> {
    let text = java_trim(s);
    if text.is_empty() {
        return Err(JError::new(NUMBER_FORMAT, "empty String"));
    }
    let bad = || JError::new(NUMBER_FORMAT, format!("For input string: \"{text}\""));
    let (negative, body) = match text.as_bytes()[0] {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let magnitude = match body {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        _ => {
            let body = body.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(body);
            if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
                parse_hex_literal(hex).ok_or_else(bad)?
            } else if is_decimal_literal(body) {
                body.parse::<f64>().map_err(|_| bad())?
            } else {
                return Err(bad());
            }
        }
    };
    Ok(if negative { -magnitude } else { magnitude })
}

/// `digits [. digits] [(e|E) [sign] digits]` with at least one mantissa digit (`1.`, `.5` included).
fn is_decimal_literal(s: &str) -> bool {
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(at) => (&s[..at], Some(&s[at + 1..])),
        None => (s, None),
    };
    let mut dots = 0;
    let mut digits = 0;
    for c in mantissa.chars() {
        match c {
            '0'..='9' => digits += 1,
            '.' => dots += 1,
            _ => return false,
        }
    }
    if digits == 0 || dots > 1 {
        return false;
    }
    match exponent {
        None => true,
        Some(e) => {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
        }
    }
}

/// The part after `0x`: hex digits with an optional point, then the mandatory `p` exponent (decimal, signed).
/// Rounded to nearest, ties to even, subnormals included.
fn parse_hex_literal(s: &str) -> Option<f64> {
    let at = s.find(['p', 'P'])?;
    let (mantissa, exponent) = (&s[..at], &s[at + 1..]);
    let exp_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    if exp_digits.is_empty() || !exp_digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Saturate huge exponents: anything this large is already infinity or zero.
    let mut binary_exp: i64 = exp_digits.chars().fold(0i64, |acc, c| {
        (acc * 10 + (c as i64 - '0' as i64)).min(1 << 40)
    });
    if exponent.starts_with('-') {
        binary_exp = -binary_exp;
    }
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(dot) => (&mantissa[..dot], &mantissa[dot + 1..]),
        None => (mantissa, ""),
    };
    if int_part.len() + frac_part.len() == 0 || frac_part.contains('.') {
        return None;
    }
    // Keep up to 15 significant hex digits (60 bits); later non-zero digits only matter as a sticky bit.
    let mut value: u64 = 0;
    let mut kept = 0;
    let mut sticky = false;
    for (i, c) in int_part.chars().chain(frac_part.chars()).enumerate() {
        let d = c.to_digit(16)? as u64;
        let in_fraction = i >= int_part.len();
        if value == 0 && d == 0 {
            if in_fraction {
                binary_exp -= 4;
            }
            continue;
        }
        if kept < 15 {
            value = value * 16 + d;
            kept += 1;
            if in_fraction {
                binary_exp -= 4;
            }
        } else {
            sticky |= d != 0;
            if !in_fraction {
                binary_exp += 4;
            }
        }
    }
    if value == 0 {
        return Some(0.0);
    }
    Some(scale_binary(value, binary_exp, sticky))
}

/// `value * 2^exp` (plus a sticky fraction below `value`'s last bit), rounded to a double.
fn scale_binary(value: u64, exp: i64, sticky: bool) -> f64 {
    let bits = 64 - value.leading_zeros() as i64;
    let top = exp + bits - 1;
    if top > 1023 {
        return f64::INFINITY;
    }
    // Position of the last bit a double can keep at this magnitude.
    let last = (top - 52).max(-1074);
    let shift = last - exp;
    let kept = if shift <= 0 {
        value << (-shift)
    } else if shift >= 64 {
        // Everything is below the last bit: round to zero or the smallest step.
        let half_or_more = shift == 64 && value >> 63 == 1;
        let above_half = half_or_more && (value << 1 != 0 || sticky);
        u64::from(above_half)
    } else {
        let q = value >> shift;
        let rest = value & ((1u64 << shift) - 1);
        let half = 1u64 << (shift - 1);
        let round_up = rest > half || (rest == half && (sticky || q & 1 == 1));
        q + u64::from(round_up)
    };
    let mut result = kept as f64;
    // Multiply by 2^last in steps that are each exact.
    let mut e = last;
    while e > 0 {
        let step = e.min(1000);
        result *= f64::powi(2.0, step as i32);
        e -= step;
    }
    while e < 0 {
        let step = (-e).min(1000);
        result *= f64::powi(2.0, -(step as i32));
        e += step;
    }
    result
}

// -------------------------------------------------------------------------------------------------------------
// ChoiceFormat

/// `java.text.ChoiceFormat`: limits in (normally) ascending order, each with the text used from that limit up to
/// the next one.
#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceFormat {
    limits: Vec<f64>,
    formats: Vec<String>,
}

/// The least double above `d` (`ChoiceFormat.nextDouble`): `+∞` and NaN stay, `-0.0` and `0.0` step to
/// `Double.MIN_VALUE`.
pub fn next_double(d: f64) -> f64 {
    d.next_up()
}

/// The greatest double below `d` (`ChoiceFormat.previousDouble`): `-∞` and NaN stay.
pub fn previous_double(d: f64) -> f64 {
    d.next_down()
}

/// The pattern characters that end a limit (`#`, `<`, `≤`).
fn is_relation(c: char) -> bool {
    matches!(c, '#' | '<' | '\u{2264}')
}

/// A limit's text: the infinity signs, else `Double.parseDouble`.
fn parse_limit(text: &str) -> Result<f64, JError> {
    match text {
        "\u{221e}" => Ok(f64::INFINITY),
        "-\u{221e}" => Ok(f64::NEG_INFINITY),
        _ => java_parse_double(text),
    }
}

impl ChoiceFormat {
    /// `new ChoiceFormat(pattern)`: `limit relation text` pieces separated by `|`. A piece without a relation is
    /// dropped (JDK 26 drops a leading empty piece; recorded fixture). Quotes make `#<≤|` literal; `''` is a quote.
    pub fn new(pattern: &str) -> Result<ChoiceFormat, JError> {
        let mut result = ChoiceFormat {
            limits: Vec::new(),
            formats: Vec::new(),
        };
        let mut limit_text = String::new();
        let mut text = String::new();
        // The limit of the piece whose text is being read; `None` while its limit is being read.
        let mut current: Option<f64> = None;
        let mut quoted = false;
        let mut chars = pattern.chars().peekable();
        while let Some(c) = chars.next() {
            let reading_text = current.is_some();
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    if reading_text {
                        text.push('\'')
                    } else {
                        limit_text.push('\'')
                    }
                } else {
                    quoted = !quoted;
                }
                continue;
            }
            if quoted {
                if reading_text {
                    text.push(c)
                } else {
                    limit_text.push(c)
                }
            } else if is_relation(c) {
                if limit_text.is_empty() {
                    return Err(illegal(
                        "Each interval must contain a number before a format",
                    ));
                }
                let mut limit = parse_limit(&limit_text)?;
                if c == '<' {
                    limit = next_double(limit);
                }
                if result.limits.last().is_some_and(|&last| limit <= last) {
                    return Err(illegal(
                        "Incorrect order of intervals, must be in ascending order",
                    ));
                }
                limit_text.clear();
                current = Some(limit);
            } else if c == '|' {
                if let Some(limit) = current.take() {
                    result.limits.push(limit);
                    result.formats.push(std::mem::take(&mut text));
                }
                limit_text.clear();
            } else if reading_text {
                text.push(c);
            } else {
                limit_text.push(c);
            }
        }
        if let Some(limit) = current {
            result.limits.push(limit);
            result.formats.push(text);
        }
        Ok(result)
    }

    /// `new ChoiceFormat(limits, formats)` (`setChoices`): the order of the limits is not checked.
    pub fn with_choices(limits: &[f64], formats: &[&str]) -> Result<ChoiceFormat, JError> {
        if limits.len() != formats.len() {
            return Err(illegal(
                "Array and limit arrays must be of the same length.",
            ));
        }
        Ok(ChoiceFormat {
            limits: limits.to_vec(),
            formats: formats.iter().map(|s| s.to_string()).collect(),
        })
    }

    /// `getLimits()`.
    pub fn limits(&self) -> &[f64] {
        &self.limits
    }

    /// `getFormats()`.
    pub fn formats(&self) -> &[String] {
        &self.formats
    }

    /// `format(double)`: the text of the last limit not above `number`, scanning from the first and stopping at
    /// the first limit above it; the first text when `number` is below the first limit or NaN.
    pub fn format_f64(&self, number: f64) -> Result<String, JError> {
        if self.limits.is_empty() || self.formats.is_empty() {
            return Err(JError::new(
                "java.lang.ArrayIndexOutOfBoundsException",
                "Index 0 out of bounds for length 0",
            ));
        }
        let mut chosen = 0;
        for (i, &limit) in self.limits.iter().enumerate() {
            if number >= limit {
                chosen = i;
            } else {
                break;
            }
        }
        Ok(self.formats[chosen.min(self.formats.len() - 1)].clone())
    }

    /// `format(Object)`: any `Number`, by its `double` value.
    pub fn format(&self, value: &JValue) -> Result<String, JError> {
        match number_as_f64(value) {
            Some(d) => self.format_f64(d),
            None => Err(illegal("Cannot format given Object as a Number")),
        }
    }

    /// `toPattern()`: each limit written either as itself with `#` or as the double below it with `<`, whichever
    /// is strictly nearer to a whole number keeps `#` (ties and non-finite limits take `<`); `±∞` limits are
    /// written as `∞`/`-∞`; a text containing `#<≤|` is quoted whole, quotes are doubled.
    pub fn to_pattern(&self) -> String {
        let mut out = String::new();
        for (i, (&limit, text)) in self.limits.iter().zip(&self.formats).enumerate() {
            if i > 0 {
                out.push('|');
            }
            let below = previous_double(limit);
            let (value, relation) = if distance_to_integer(limit) < distance_to_integer(below) {
                (limit, '#')
            } else {
                (below, '<')
            };
            // An infinite limit, or an infinite double below it, is written as a sign (`-MAX_VALUE` written as
            // `-∞<` is JDK 26's form, recorded in the fixtures; JDK 21 writes `-Infinity<`).
            if limit == f64::INFINITY {
                out.push('\u{221e}');
            } else if limit == f64::NEG_INFINITY || value == f64::NEG_INFINITY {
                out.push_str("-\u{221e}");
            } else {
                out.push_str(&crate::number::double_to_string(value));
            }
            out.push(relation);
            let wrap = text.chars().any(|c| is_relation(c) || c == '|');
            if wrap {
                out.push('\'');
            }
            for c in text.chars() {
                if c == '\'' {
                    out.push_str("''");
                } else {
                    out.push(c);
                }
            }
            if wrap {
                out.push('\'');
            }
        }
        out
    }
}

/// How far `d` is from the nearest whole number (ties to even); NaN for infinities and NaN.
fn distance_to_integer(d: f64) -> f64 {
    (d - d.round_ties_even()).abs()
}

/// `Number.doubleValue()` of a boxed number.
fn number_as_f64(value: &JValue) -> Option<f64> {
    Some(match value {
        JValue::Byte(b) => *b as f64,
        JValue::Short(s) => *s as f64,
        JValue::Int(i) => *i as f64,
        JValue::Long(l) => *l as f64,
        JValue::Float(f) => *f as f64,
        JValue::Double(d) => *d,
        JValue::BigInteger(s) | JValue::BigDecimal(s) => s.parse::<f64>().ok()?,
        _ => return None,
    })
}

fn is_number(value: &JValue) -> bool {
    matches!(
        value,
        JValue::Byte(_)
            | JValue::Short(_)
            | JValue::Int(_)
            | JValue::Long(_)
            | JValue::Float(_)
            | JValue::Double(_)
            | JValue::BigInteger(_)
            | JValue::BigDecimal(_)
    )
}

// -------------------------------------------------------------------------------------------------------------
// MessageFormat

/// The format an argument element names: `number` (a `DecimalFormat`), `date`/`time` (a `SimpleDateFormat`) or
/// `choice`.
#[derive(Clone)]
pub enum SubFormat {
    /// `number`.
    Number(Box<DecimalFormat>),
    /// `date` or `time`.
    Date(Box<SimpleDateFormat>),
    /// `choice`.
    Choice(ChoiceFormat),
}

impl fmt::Debug for SubFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubFormat::Number(d) => write!(f, "Number({:?})", d.to_pattern()),
            SubFormat::Date(d) => write!(f, "Date({:?})", d.to_pattern()),
            SubFormat::Choice(c) => write!(f, "Choice({:?})", c.to_pattern()),
        }
    }
}

/// One part of a message pattern.
#[derive(Debug, Clone)]
enum Piece {
    Text(String),
    Argument {
        index: usize,
        format: Option<SubFormat>,
    },
}

/// `java.text.MessageFormat` for one locale; dates are formatted in the default zone (GMT).
#[derive(Clone)]
pub struct MessageFormat {
    locale: Locale,
    pieces: Vec<Piece>,
}

impl fmt::Debug for MessageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageFormat")
            .field("locale", &self.locale)
            .field("pattern", &self.to_pattern())
            .finish()
    }
}

/// The highest argument index plus one that a pattern may name (recorded on JDK 26).
const ARGUMENT_INDEX_LIMIT: i64 = 10000;

impl MessageFormat {
    /// `new MessageFormat(pattern, locale)`.
    pub fn new(pattern: &str, locale: &Locale) -> Result<MessageFormat, JError> {
        let mut f = MessageFormat {
            locale: locale.clone(),
            pieces: Vec::new(),
        };
        f.apply_pattern(pattern)?;
        Ok(f)
    }

    /// `applyPattern(pattern)`. On an error the previous pattern stays.
    pub fn apply_pattern(&mut self, pattern: &str) -> Result<(), JError> {
        self.pieces = self.parse(pattern)?;
        Ok(())
    }

    fn parse(&self, pattern: &str) -> Result<Vec<Piece>, JError> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut quoted = false;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    text.push('\'');
                    i += 2;
                } else {
                    quoted = !quoted;
                    i += 1;
                }
                continue;
            }
            if c == '{' && !quoted {
                let (parts, next) = scan_element(&chars, i + 1)?;
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(self.argument(&parts)?);
                i = next;
                continue;
            }
            text.push(c);
            i += 1;
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(pieces)
    }

    /// An argument element from its index, type and style texts.
    fn argument(&self, parts: &[String; 3]) -> Result<Piece, JError> {
        let [index_text, type_text, style] = parts;
        let index = parse_java_int(index_text)
            .ok_or_else(|| illegal(format!("can't parse argument number: {index_text}")))?;
        if index < 0 {
            return Err(illegal(format!("negative argument number: {index}")));
        }
        if index >= ARGUMENT_INDEX_LIMIT {
            return Err(illegal(format!(
                "{index} exceeds the ArgumentIndex implementation limit"
            )));
        }
        let type_name = java_trim(type_text);
        let keyword = java_trim(style).to_lowercase();
        let locale = &self.locale;
        let format = match type_name.to_lowercase().as_str() {
            "" => None,
            "number" => Some(SubFormat::Number(Box::new(match keyword.as_str() {
                "" => NumberFormat::number_instance(locale),
                "currency" => NumberFormat::currency_instance(locale),
                "percent" => NumberFormat::percent_instance(locale),
                "integer" => NumberFormat::integer_instance(locale),
                _ => DecimalFormat::with_locale(style, locale)?,
            }))),
            kind @ ("date" | "time") => {
                let zone = TimeZone::default_zone();
                let style_code = match keyword.as_str() {
                    "" | "medium" => Some(MEDIUM),
                    "short" => Some(SHORT),
                    "long" => Some(LONG),
                    "full" => Some(FULL),
                    _ => None,
                };
                Some(SubFormat::Date(Box::new(match (style_code, kind) {
                    (Some(s), "date") => DateFormat::date_instance(s, locale, &zone)?,
                    (Some(s), _) => DateFormat::time_instance(s, locale, &zone)?,
                    (None, _) => SimpleDateFormat::new(style, locale, &zone)?,
                })))
            }
            "choice" => Some(SubFormat::Choice(ChoiceFormat::new(style)?)),
            _ => return Err(illegal(format!("unknown format type: {type_name}"))),
        };
        Ok(Piece::Argument {
            index: index as usize,
            format,
        })
    }

    /// `format(Object[])`: a missing argument stays `{n}`, `null` is `null`; with a subformat the argument goes
    /// through it (a choice result containing `{` is formatted again as a pattern with the same arguments);
    /// without one numbers use the locale's number instance, dates its short date-time instance, strings stay
    /// and anything else is `toString()`.
    pub fn format(&self, args: &[JValue]) -> Result<String, JError> {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Argument { index, format } => match args.get(*index) {
                    None => {
                        out.push('{');
                        out.push_str(&index.to_string());
                        out.push('}');
                    }
                    Some(JValue::Null) => out.push_str("null"),
                    Some(arg) => out.push_str(&self.format_argument(arg, format.as_ref(), args)?),
                },
            }
        }
        Ok(out)
    }

    fn format_argument(
        &self,
        arg: &JValue,
        format: Option<&SubFormat>,
        args: &[JValue],
    ) -> Result<String, JError> {
        match format {
            Some(SubFormat::Number(f)) => f.format(arg),
            Some(SubFormat::Date(f)) => {
                let millis = date_or_number_millis(arg)
                    .ok_or_else(|| illegal("Cannot format given Object as a Date"))?;
                Ok(f.clone().format(millis))
            }
            Some(SubFormat::Choice(f)) => {
                let s = f.format(arg)?;
                if s.contains('{') {
                    MessageFormat::new(&s, &self.locale)?.format(args)
                } else {
                    Ok(s)
                }
            }
            None => {
                if is_number(arg) {
                    NumberFormat::number_instance(&self.locale).format(arg)
                } else if let Some(millis) = super::objects::date_millis(arg) {
                    Ok(DateFormat::date_time_instance(
                        SHORT,
                        SHORT,
                        &self.locale,
                        &TimeZone::default_zone(),
                    )?
                    .format(millis))
                } else if let JValue::Str(s) = arg {
                    Ok(s.to_string())
                } else {
                    Ok(arg.to_java_string())
                }
            }
        }
    }

    /// `toPattern()`: literal text with `{` quoted and quotes doubled; each argument as `{n}`, `{n,type}` or
    /// `{n,type,style}`, where a subformat equal to one of the locale's standard instances is named by its
    /// keyword and any other by its own pattern (unquoted braces in it quoted).
    pub fn to_pattern(&self) -> String {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(t) => quote_text(t, &mut out),
                Piece::Argument { index, format } => {
                    out.push('{');
                    out.push_str(&index.to_string());
                    match format {
                        None => {}
                        Some(SubFormat::Number(f)) => out.push_str(&self.number_element(f)),
                        Some(SubFormat::Date(f)) => out.push_str(&self.date_element(f)),
                        Some(SubFormat::Choice(f)) => {
                            out.push_str(",choice,");
                            quote_braces(&f.to_pattern(), &mut out);
                        }
                    }
                    out.push('}');
                }
            }
        }
        out
    }

    fn number_element(&self, f: &DecimalFormat) -> String {
        let l = &self.locale;
        let named = [
            (NumberFormat::number_instance(l), ",number"),
            (NumberFormat::currency_instance(l), ",number,currency"),
            (NumberFormat::percent_instance(l), ",number,percent"),
            (NumberFormat::integer_instance(l), ",number,integer"),
        ];
        for (standard, name) in &named {
            if same_decimal_format(f, standard) {
                return name.to_string();
            }
        }
        let mut out = String::from(",number,");
        quote_braces(&f.to_pattern(), &mut out);
        out
    }

    fn date_element(&self, f: &SimpleDateFormat) -> String {
        let pattern = f.to_pattern();
        let zone = TimeZone::default_zone();
        let styles = [
            (MEDIUM, ""),
            (SHORT, ",short"),
            (LONG, ",long"),
            (FULL, ",full"),
        ];
        for (kind, is_date) in [("date", true), ("time", false)] {
            for (style, suffix) in styles {
                let standard = if is_date {
                    DateFormat::date_instance(style, &self.locale, &zone)
                } else {
                    DateFormat::time_instance(style, &self.locale, &zone)
                };
                if standard.is_ok_and(|s| s.to_pattern() == pattern) {
                    return format!(",{kind}{suffix}");
                }
            }
        }
        let mut out = String::from(",date,");
        quote_braces(&pattern, &mut out);
        out
    }
}

/// Whether two decimal formats have the same observable settings (`DecimalFormat.equals` for formats built over
/// the same locale's symbols).
fn same_decimal_format(a: &DecimalFormat, b: &DecimalFormat) -> bool {
    a.to_pattern() == b.to_pattern()
        && a.positive_prefix() == b.positive_prefix()
        && a.positive_suffix() == b.positive_suffix()
        && a.negative_prefix() == b.negative_prefix()
        && a.negative_suffix() == b.negative_suffix()
        && a.multiplier() == b.multiplier()
        && a.grouping_size() == b.grouping_size()
        && a.is_grouping_used() == b.is_grouping_used()
        && a.is_decimal_separator_always_shown() == b.is_decimal_separator_always_shown()
        && a.is_parse_big_decimal() == b.is_parse_big_decimal()
        && a.is_parse_integer_only() == b.is_parse_integer_only()
        && a.rounding_mode() == b.rounding_mode()
        && a.maximum_integer_digits() == b.maximum_integer_digits()
        && a.minimum_integer_digits() == b.minimum_integer_digits()
        && a.maximum_fraction_digits() == b.maximum_fraction_digits()
        && a.minimum_fraction_digits() == b.minimum_fraction_digits()
}

/// Reads an argument element after its `{`: index, type and style separated by the first two unquoted commas,
/// nested braces balanced, quotes kept (they are the subformat's). Returns the parts and the index after `}`.
fn scan_element(chars: &[char], start: usize) -> Result<([String; 3], usize), JError> {
    let mut parts: [String; 3] = Default::default();
    let mut part = 0;
    let mut depth = 0;
    let mut quoted = false;
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c == '\'' {
            quoted = !quoted;
        } else if !quoted {
            match c {
                ',' if part < 2 && depth == 0 => {
                    part += 1;
                    continue;
                }
                '{' => depth += 1,
                '}' if depth == 0 => return Ok((parts, i)),
                '}' => depth -= 1,
                _ => {}
            }
        }
        parts[part].push(c);
    }
    Err(illegal("Unmatched braces in the pattern."))
}

/// `Integer.parseInt`: optional sign, then decimal digits (any Unicode decimal digit), within `int` range.
fn parse_java_int(s: &str) -> Option<i64> {
    let (negative, digits) = match s.chars().next()? {
        '-' => (true, &s[1..]),
        '+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for c in digits.chars() {
        let d = c
            .to_digit(10)
            .or_else(|| super::decimal::unicode_digit(c))? as i64;
        value = value * 10 + d;
        if value > i32::MAX as i64 + 1 {
            return None;
        }
    }
    let value = if negative { -value } else { value };
    (i32::MIN as i64..=i32::MAX as i64)
        .contains(&value)
        .then_some(value)
}

/// The milliseconds a date subformat formats: a `Date`'s time, or a number's `longValue()`.
fn date_or_number_millis(arg: &JValue) -> Option<i64> {
    super::objects::date_millis(arg).or_else(|| {
        if is_number(arg) {
            super::number_long_value(arg)
        } else {
            None
        }
    })
}

/// Literal text in `toPattern` form: a `{` opens a quote that stays open over further braces and quotes and
/// closes before the next other char; a quote is always doubled.
fn quote_text(text: &str, out: &mut String) {
    let mut open = false;
    for c in text.chars() {
        match c {
            '{' => {
                if !open {
                    out.push('\'');
                    open = true;
                }
                out.push('{');
            }
            '\'' => out.push_str("''"),
            _ => {
                if open {
                    out.push('\'');
                    open = false;
                }
                out.push(c);
            }
        }
    }
    if open {
        out.push('\'');
    }
}

/// A subformat pattern in `toPattern` form: braces outside its quoted runs are quoted one by one.
fn quote_braces(pattern: &str, out: &mut String) {
    let mut quoted = false;
    for c in pattern.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                out.push(c);
            }
            '{' | '}' if !quoted => {
                out.push('\'');
                out.push(c);
                out.push('\'');
            }
            _ => out.push(c),
        }
    }
}

/// `MessageFormat.format(pattern, args)` in `locale`.
pub fn message_format(pattern: &str, args: &[JValue], locale: &Locale) -> Result<String, JError> {
    MessageFormat::new(pattern, locale)?.format(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(p: &str) -> String {
        match ChoiceFormat::new(p) {
            Ok(f) => f.to_pattern(),
            Err(e) => e.to_string(),
        }
    }

    /// Recorded on JDK 21.0.12.1 (`probes/choice.out`).
    #[test]
    fn choice_to_pattern_picks_the_rounder_neighbour() {
        for (p, want) in [
            ("0.7#a", "0.7#a"),
            ("0.3#a", "0.29999999999999993<a"),
            ("-0.1#a", "-0.1#a"),
            ("1e22#a", "9.999999999999998E21<a"),
            ("4.9E-324#a", "0.0<a"),
            ("NaN#a", "NaN<a"),
            ("Infinity#a", "\u{221e}<a"),
            ("1<a|2<b", "1.0<a|2.0<b"),
            ("1#a'|'b|2#c", "1.0#'a|b'|2.0#c"),
            ("1#'''#'''", "1.0#'''#'''"),
        ] {
            assert_eq!(choice(p), want, "{p}");
        }
        let f = ChoiceFormat::with_choices(&[-f64::MAX, -4.9E-324], &["a", "b"]).unwrap();
        assert_eq!(f.to_pattern(), "-\u{221e}<a|-4.9E-324#b");
    }

    #[test]
    fn choice_parse_errors_and_limits() {
        for (p, want) in [
            (" #a", "java.lang.NumberFormatException: empty String"),
            (
                " 0x1 #a",
                "java.lang.NumberFormatException: For input string: \"0x1\"",
            ),
            (
                " \u{221e} #a",
                "java.lang.NumberFormatException: For input string: \"\u{221e}\"",
            ),
            (
                "infinity#a",
                "java.lang.NumberFormatException: For input string: \"infinity\"",
            ),
            (
                "1e#a",
                "java.lang.NumberFormatException: For input string: \"1e\"",
            ),
            (
                "1#a\u{2264}",
                "java.lang.IllegalArgumentException: Each interval must contain a number before a format",
            ),
            (
                "-0#a|0#b",
                "java.lang.IllegalArgumentException: Incorrect order of intervals, must be in ascending order",
            ),
            ("0x1.8p1#a", "3.0#a"),
            ("1.5e1#a", "15.0#a"),
            ("-NaN#a|1#b", "NaN<a|1.0#b"),
            ("\t1\n#a", "1.0#a"),
        ] {
            assert_eq!(choice(p), want, "{p:?}");
        }
        assert_eq!(java_parse_double("0x1p-1074").unwrap(), 4.9E-324);
        assert_eq!(
            java_parse_double("0x1.fffffffffffffp1023").unwrap(),
            f64::MAX
        );
        assert_eq!(java_parse_double("0x1p1024").unwrap(), f64::INFINITY);
        assert_eq!(java_parse_double("-0x.8p1d").unwrap(), -1.0);
    }

    #[test]
    fn choice_format_scans_unordered_limits() {
        let f = ChoiceFormat::with_choices(&[2.0, 1.0], &["a", "b"]).unwrap();
        assert_eq!(
            [1.5, 0.0, 3.0].map(|d| f.format_f64(d).unwrap()).concat(),
            "aab"
        );
        assert!(ChoiceFormat::with_choices(&[1.0, 2.0], &["a"]).is_err());
        assert_eq!(next_double(-4.9E-324).to_bits(), (-0.0f64).to_bits());
        assert_eq!(previous_double(f64::INFINITY), f64::MAX);
        assert_eq!(next_double(f64::INFINITY), f64::INFINITY);
    }

    /// Recorded on JDK 21.0.12.1 (`probes/message.out`); JDK 26 agrees where the fixtures overlap.
    #[test]
    fn message_corners() {
        let us = Locale::us();
        let run = |p: &str, args: &[JValue]| match MessageFormat::new(p, &us) {
            Ok(m) => format!(
                "{} | {}",
                m.to_pattern(),
                m.format(args).unwrap_or_else(|e| e.to_string())
            ),
            Err(e) => e.to_string(),
        };
        for (p, args, want) in [
            ("'{{'", vec![], "'{{' | {{"),
            ("'{{}}'", vec![], "'{{'}} | {{}}"),
            ("'{'''", vec![], "'{''' | {'"),
            ("x'{'y'{'z", vec![], "x'{'y'{'z | x{y{z"),
            (
                "{0, number, integer}",
                vec![JValue::Double(1234.5)],
                "{0,number,integer} | 1,234",
            ),
            (
                "{0,number,#,##0.###}",
                vec![JValue::Double(1.5)],
                "{0,number} | 1.5",
            ),
            (
                "{0,number,#,##0}",
                vec![JValue::Double(1.5)],
                "{0,number,#,##0} | 2",
            ),
            (
                "{0,date,M/d/yy}",
                vec![JValue::Long(0)],
                "{0,date,short} | 1/1/70",
            ),
            (
                "{0,time,MMM d, y}",
                vec![JValue::Long(0)],
                "{0,date} | Jan 1, 1970",
            ),
            (
                "{+1}{01}{-0}",
                vec![JValue::str("a"), JValue::str("b")],
                "{1}{1}{0} | bba",
            ),
            (
                "{0 }",
                vec![],
                "java.lang.IllegalArgumentException: can't parse argument number: 0 ",
            ),
            (
                "{}",
                vec![],
                "java.lang.IllegalArgumentException: can't parse argument number: ",
            ),
            (
                "{0, foo }",
                vec![],
                "java.lang.IllegalArgumentException: unknown format type: foo",
            ),
            (
                "{0,choice,0#a|1#b'|'c}",
                vec![JValue::Int(1)],
                "{0,choice,0.0#a|1.0#'b|c'} | b|c",
            ),
            ("{0,number,0}", vec![JValue::Null], "{0,number,#0} | null"),
            (
                "{0,date}",
                vec![JValue::str("s")],
                "{0,date} | java.lang.IllegalArgumentException: Cannot format given Object as a Date",
            ),
            ("{0}", vec![JValue::Float(1.5)], "{0} | 1.5"),
            ("{0}", vec![JValue::Char('c')], "{0} | c"),
            ("{0}", vec![JValue::Bool(true)], "{0} | true"),
        ] {
            assert_eq!(run(p, &args), want, "{p}");
        }
        // Formatting only: JDK 21 writes these subformat patterns without quoting their braces, JDK 26 quotes them.
        for (p, args, want) in [
            (
                "{0,choice,0#''{1}''}",
                vec![JValue::Int(0), JValue::str("Q")],
                "{1}",
            ),
            (
                "{0,choice,0#a'{'b}",
                vec![JValue::Int(0)],
                "java.lang.IllegalArgumentException: Unmatched braces in the pattern.",
            ),
            ("{0,date,{'}'}}", vec![JValue::Long(0)], "{}}"),
        ] {
            let got = MessageFormat::new(p, &us)
                .unwrap()
                .format(&args)
                .unwrap_or_else(|e| e.to_string());
            assert_eq!(got, want, "{p}");
        }
    }
}
