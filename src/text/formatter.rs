//! `java.util.Formatter` as `String.format(Locale, String, Object...)` uses it.
//!
//! Written from the Java SE `Formatter` specification, the recorded fixtures (`tests/fixtures/*/formatter.json`)
//! and observations of a JDK 21, without OpenJDK source.
//!
//! The format string is first read whole into literal text and `Spec`s, each checked on its own (flags,
//! width, precision against its conversion); only then are the arguments taken and printed, so a malformed
//! specifier late in the string fails before anything is formatted. Numbers follow the specification's
//! localisation algorithm: the locale's zero digit, grouping separator and size, and decimal separator.
//! Floating-point values are rounded `HALF_UP` from the digits of `Double.toString` (a `float` is widened to
//! `double` first), `BigDecimal`s from their exact digits; `%a` rounds the binary significand half-even.

use num_bigint::{BigInt, Sign};

use super::digits::BigDec;
use super::symbols::{DateFormatSymbols, DecimalFormatSymbols};
use super::{Locale, NumberFormat};
use crate::lang::{JError, JValue, java_hash_code};
use crate::number::double_to_string;
use crate::util::calendar::{
    AM_PM, DAY_OF_MONTH, DAY_OF_WEEK, DAY_OF_YEAR, DST_OFFSET, HOUR, HOUR_OF_DAY, MINUTE, MONTH,
    SECOND, YEAR, ZONE_OFFSET,
};
use crate::util::{GregorianCalendar, TimeZone};

const LEFT: u8 = 1;
const ALT: u8 = 2;
const PLUS: u8 = 4;
const SPACE: u8 = 8;
const ZERO: u8 = 16;
const GROUP: u8 = 32;
const PAREN: u8 = 64;
const PREVIOUS: u8 = 128;

/// The flags in the order the specification lists them, which is also how messages print them.
const FLAG_CHARS: [(u8, char); 8] = [
    (LEFT, '-'),
    (ALT, '#'),
    (PLUS, '+'),
    (SPACE, ' '),
    (ZERO, '0'),
    (GROUP, ','),
    (PAREN, '('),
    (PREVIOUS, '<'),
];

fn flag_text(flags: u8) -> String {
    FLAG_CHARS
        .iter()
        .filter(|(f, _)| flags & f != 0)
        .map(|(_, c)| *c)
        .collect()
}

/// `String.format(locale, format, args)`.
pub fn string_format(locale: &Locale, format: &str, args: &[JValue]) -> Result<String, JError> {
    let pieces = parse(format)?;
    let mut out = String::new();
    let mut ordinary = 0usize;
    let mut last: Option<usize> = None;
    let mut ctx = Context {
        locale,
        numbers: None,
    };
    for piece in &pieces {
        let spec = match piece {
            Piece::Text(t) => {
                out.push_str(t);
                continue;
            }
            Piece::Spec(s) => s,
        };
        match spec.conversion {
            '%' => {
                out.push_str(&justify(spec, "%".into()));
                continue;
            }
            'n' => {
                out.push('\n');
                continue;
            }
            _ => {}
        }
        let index = match spec.index {
            Index::Explicit(i) => Some(i - 1),
            Index::Previous => last,
            Index::Ordinary => {
                ordinary += 1;
                Some(ordinary - 1)
            }
        };
        let Some(index) = index.filter(|i| *i < args.len()) else {
            return Err(err(
                "java.util.MissingFormatArgumentException",
                format!("Format specifier '{spec}'"),
            ));
        };
        last = Some(index);
        out.push_str(&ctx.print(spec, &args[index])?);
    }
    Ok(out)
}

fn err(class: &str, message: impl Into<String>) -> JError {
    JError::new(class, message)
}

fn mismatch(conversion: char, flags: u8) -> JError {
    err(
        "java.util.FormatFlagsConversionMismatchException",
        format!("Conversion = {conversion}, Flags = {}", flag_text(flags)),
    )
}

fn conversion_error(conversion: char, arg: &JValue) -> JError {
    err(
        "java.util.IllegalFormatConversionException",
        format!("{conversion} != {}", arg.class_name()),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Index {
    Ordinary,
    Explicit(usize),
    Previous,
}

/// One format specifier: `%[index$][flags][width][.precision]conversion`, the conversion lower-cased with
/// `upper` recording an upper-case one, and the second char of a date/time conversion in `date`.
#[derive(Debug, Clone)]
struct Spec {
    index: Index,
    explicit_index: Option<usize>,
    flags: u8,
    width: Option<i32>,
    precision: Option<i32>,
    conversion: char,
    upper: bool,
    date: Option<char>,
}

impl Spec {
    fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
}

impl std::fmt::Display for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Observed order (JDK 21): flags before the argument index, which `<` hides (`%1$-#.2e` prints as
        // `%-#1$.2e`, `%1$<s` as `%<s`).
        f.write_str("%")?;
        f.write_str(&flag_text(self.flags))?;
        if let Some(i) = self.explicit_index.filter(|_| !self.has(PREVIOUS)) {
            write!(f, "{i}$")?;
        }
        if let Some(w) = self.width {
            write!(f, "{w}")?;
        }
        if let Some(p) = self.precision {
            write!(f, ".{p}")?;
        }
        let case = |c: char| {
            if self.upper {
                c.to_ascii_uppercase()
            } else {
                c
            }
        };
        match self.date {
            Some(d) => write!(f, "{}{}", case('t'), case(d)),
            None => write!(f, "{}", case(self.conversion)),
        }
    }
}

enum Piece {
    Text(String),
    Spec(Spec),
}

const DATE_CONVERSIONS: &str = "HIklMSLNpzZsQBbhAaCYyjmdeRTrDFc";

/// Reads the whole format string, checking each specifier as it is read.
fn parse(format: &str) -> Result<Vec<Piece>, JError> {
    let chars: Vec<char> = format.chars().collect();
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            text.push(chars[i]);
            i += 1;
            continue;
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(std::mem::take(&mut text)));
        }
        let (spec, next) = parse_spec(&chars, i + 1)?;
        check(&spec)?;
        pieces.push(Piece::Spec(spec));
        i = next;
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

fn unknown(conversion: impl Into<String>) -> JError {
    err(
        "java.util.UnknownFormatConversionException",
        format!("Conversion = '{}'", conversion.into()),
    )
}

fn digits_at(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// A specifier starting after its `%`; when its text does not have the specifier's shape the error names the
/// char after the `%` (or `%` itself at the end).
fn parse_spec(chars: &[char], start: usize) -> Result<(Spec, usize), JError> {
    let malformed = || unknown(chars.get(start).copied().unwrap_or('%').to_string());
    let number = |from: usize, to: usize| -> Option<i32> {
        let text: String = chars[from..to].iter().collect();
        text.parse::<i64>().ok().and_then(|v| i32::try_from(v).ok())
    };
    let mut i = start;
    let mut index = Index::Ordinary;
    let mut explicit_index = None;
    let end = digits_at(chars, i);
    if end > i && chars.get(end) == Some(&'$') {
        match number(i, end) {
            None => {
                return Err(err(
                    "java.util.IllegalFormatArgumentIndexException",
                    "Format argument index: (not representable as int)",
                ));
            }
            Some(0) => {
                return Err(err(
                    "java.util.IllegalFormatArgumentIndexException",
                    "Illegal format argument index = 0",
                ));
            }
            Some(n) => {
                index = Index::Explicit(n as usize);
                explicit_index = Some(n as usize);
            }
        }
        i = end + 1;
    }
    let mut flags = 0u8;
    while let Some(&(f, c)) = chars
        .get(i)
        .and_then(|c| FLAG_CHARS.iter().find(|(_, fc)| fc == c))
    {
        if flags & f != 0 {
            return Err(err(
                "java.util.DuplicateFormatFlagsException",
                format!("Flags = '{c}'"),
            ));
        }
        flags |= f;
        i += 1;
    }
    if flags & PREVIOUS != 0 {
        index = Index::Previous;
    }
    let mut width = None;
    let end = digits_at(chars, i);
    if end > i {
        width = Some(number(i, end).unwrap_or(i32::MIN));
        i = end;
    }
    let mut precision = None;
    if chars.get(i) == Some(&'.') {
        let end = digits_at(chars, i + 1);
        if end == i + 1 {
            return Err(malformed());
        }
        precision = Some(number(i + 1, end).unwrap_or(i32::MIN));
        i = end;
    }
    let is_conversion = |c: Option<&char>| c.is_some_and(|c| c.is_ascii_alphabetic() || *c == '%');
    let Some(&c) = chars.get(i).filter(|c| is_conversion(Some(c))) else {
        return Err(malformed());
    };
    let (conversion, date, next) = if (c == 't' || c == 'T') && is_conversion(chars.get(i + 1)) {
        (c, Some(chars[i + 1]), i + 2)
    } else {
        (c, None, i + 1)
    };
    if let Some(w) = width.filter(|w| *w < 0) {
        return Err(err("java.util.IllegalFormatWidthException", w.to_string()));
    }
    if let Some(p) = precision.filter(|p| *p < 0) {
        return Err(err(
            "java.util.IllegalFormatPrecisionException",
            p.to_string(),
        ));
    }
    let upper = conversion.is_ascii_uppercase();
    let lower = conversion.to_ascii_lowercase();
    let known = match conversion {
        'b' | 'B' | 'h' | 'H' | 's' | 'S' | 'c' | 'C' | 'd' | 'o' | 'x' | 'X' | 'e' | 'E' | 'f'
        | 'g' | 'G' | 'a' | 'A' | '%' | 'n' => true,
        't' | 'T' => match date {
            Some(d) if DATE_CONVERSIONS.contains(d) => true,
            Some(d) => return Err(unknown(format!("{conversion}{d}"))),
            None => false,
        },
        _ => false,
    };
    if !known {
        return Err(unknown(conversion.to_string()));
    }
    Ok((
        Spec {
            index,
            explicit_index,
            flags,
            width,
            precision,
            conversion: lower,
            upper,
            date,
        },
        next,
    ))
}

/// The checks of a specifier that do not depend on its argument.
fn check(s: &Spec) -> Result<(), JError> {
    let missing_width = || {
        if (s.has(LEFT) || s.has(ZERO)) && s.width.is_none() {
            Err(err("java.util.MissingFormatWidthException", s.to_string()))
        } else {
            Ok(())
        }
    };
    let left_needs_width = || {
        if s.has(LEFT) && s.width.is_none() {
            Err(err("java.util.MissingFormatWidthException", s.to_string()))
        } else {
            Ok(())
        }
    };
    let bad = |set: u8| {
        if s.flags & set != 0 {
            Err(mismatch(s.conversion, s.flags & set))
        } else {
            Ok(())
        }
    };
    let no_precision = || match s.precision {
        Some(p) => Err(err(
            "java.util.IllegalFormatPrecisionException",
            p.to_string(),
        )),
        None => Ok(()),
    };
    // The message lists every flag given, and for an upper-case conversion a `^` after `-` (observed on JDK 21:
    // `%-#05X` fails with `Flags = '-^#0'`).
    let illegal_flags = || {
        let mut text = flag_text(s.flags);
        if s.upper {
            let at = if s.has(LEFT) { 1 } else { 0 };
            text.insert(at, '^');
        }
        err(
            "java.util.IllegalFormatFlagsException",
            format!("Flags = '{text}'"),
        )
    };
    let combinations = || {
        if (s.has(PLUS) && s.has(SPACE)) || (s.has(LEFT) && s.has(ZERO)) {
            Err(illegal_flags())
        } else {
            Ok(())
        }
    };
    match s.conversion {
        'b' | 'h' | 's' => {
            if s.conversion != 's' {
                bad(ALT)?;
            }
            left_needs_width()?;
            bad(PLUS | SPACE | ZERO | GROUP | PAREN)
        }
        'c' => {
            no_precision()?;
            bad(ALT | PLUS | SPACE | ZERO | GROUP | PAREN)?;
            left_needs_width()
        }
        'd' | 'o' | 'x' => {
            missing_width()?;
            combinations()?;
            no_precision()?;
            match s.conversion {
                'd' => bad(ALT),
                _ => bad(GROUP),
            }
        }
        'e' | 'f' | 'g' | 'a' => {
            missing_width()?;
            combinations()?;
            match s.conversion {
                'e' => bad(GROUP),
                'g' => bad(ALT),
                'a' => bad(GROUP | PAREN),
                _ => Ok(()),
            }
        }
        't' => {
            let d = s.date.expect("date conversion");
            no_precision()?;
            if s.flags & (ALT | PLUS | SPACE | ZERO | GROUP | PAREN) != 0 {
                return Err(mismatch(
                    d,
                    s.flags & (ALT | PLUS | SPACE | ZERO | GROUP | PAREN),
                ));
            }
            left_needs_width()
        }
        '%' => {
            no_precision()?;
            if s.flags & !LEFT != 0 {
                return Err(illegal_flags());
            }
            left_needs_width()
        }
        'n' => {
            no_precision()?;
            if let Some(w) = s.width {
                return Err(err("java.util.IllegalFormatWidthException", w.to_string()));
            }
            if s.flags != 0 {
                return Err(illegal_flags());
            }
            Ok(())
        }
        _ => unreachable!("checked conversion"),
    }
}

/// Length as Java counts it: UTF-16 code units.
fn java_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Pads to the width with spaces, on the left unless `-` is given.
fn justify(s: &Spec, text: String) -> String {
    let Some(w) = s.width else { return text };
    let len = java_len(&text);
    let w = w as usize;
    if len >= w {
        return text;
    }
    let pad = " ".repeat(w - len);
    if s.has(LEFT) {
        text + &pad
    } else {
        pad + &text
    }
}

fn turkic(locale: &Locale) -> bool {
    matches!(locale.language(), "tr" | "az")
}

/// `String.toUpperCase(locale)`.
fn to_upper(s: &str, locale: &Locale) -> String {
    if turkic(locale) {
        s.chars()
            .map(|c| {
                if c == 'i' {
                    "İ".to_string()
                } else {
                    c.to_uppercase().collect()
                }
            })
            .collect()
    } else {
        s.to_uppercase()
    }
}

/// `String.toLowerCase(locale)`.
fn to_lower(s: &str, locale: &Locale) -> String {
    if turkic(locale) {
        s.chars()
            .map(|c| match c {
                'I' => "ı".to_string(),
                'İ' => "i".to_string(),
                c => c.to_lowercase().collect(),
            })
            .collect()
    } else {
        s.to_lowercase()
    }
}

/// The first `n` UTF-16 code units of `s` (a pair split by the cut keeps neither half).
fn truncate(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let len = c.len_utf16();
        if used + len > n {
            break;
        }
        used += len;
        out.push(c);
    }
    out
}

/// The locale's number symbols as the localisation algorithm uses them.
struct Numbers {
    zero: char,
    grouping: char,
    decimal: char,
    group_size: usize,
}

struct Context<'a> {
    locale: &'a Locale,
    numbers: Option<Numbers>,
}

impl Context<'_> {
    fn numbers(&mut self) -> &Numbers {
        let locale = self.locale;
        self.numbers.get_or_insert_with(|| {
            let symbols = DecimalFormatSymbols::new(locale);
            let format = NumberFormat::integer_instance(locale);
            let size = if format.is_grouping_used() {
                format.grouping_size().max(0) as usize
            } else {
                0
            };
            Numbers {
                zero: symbols.zero_digit,
                grouping: symbols.grouping_separator,
                decimal: symbols.decimal_separator,
                group_size: size,
            }
        })
    }

    fn case(&self, s: &Spec, text: String) -> String {
        if s.upper {
            to_upper(&text, self.locale)
        } else {
            text
        }
    }

    /// A string result: precision cuts it, then case, then width.
    fn text(&self, s: &Spec, text: String) -> String {
        let text = match s.precision {
            Some(p) => truncate(&text, p as usize),
            None => text,
        };
        justify(s, self.case(s, text))
    }

    fn print(&mut self, s: &Spec, arg: &JValue) -> Result<String, JError> {
        match s.conversion {
            'b' => {
                let v = match arg {
                    JValue::Null => "false".to_string(),
                    JValue::Bool(b) => b.to_string(),
                    _ => "true".to_string(),
                };
                Ok(self.text(s, v))
            }
            'h' => {
                let v = match arg {
                    JValue::Null => "null".to_string(),
                    other => format!("{:x}", hash_code(other) as u32),
                };
                Ok(self.text(s, v))
            }
            's' => {
                if s.has(ALT) {
                    return Err(mismatch('s', ALT));
                }
                Ok(self.text(s, arg.to_java_string()))
            }
            _ if arg.is_null() => Ok(self.text(s, "null".into())),
            'c' => self.character(s, arg),
            'd' | 'o' | 'x' => self.integral(s, arg),
            'e' | 'f' | 'g' | 'a' => self.floating(s, arg),
            't' => self.date_time(s, arg),
            other => unreachable!("conversion {other}"),
        }
    }

    fn character(&self, s: &Spec, arg: &JValue) -> Result<String, JError> {
        let code_point = match arg {
            JValue::Char(c) => *c as i32,
            JValue::Byte(b) => *b as i32,
            JValue::Short(v) => *v as i32,
            JValue::Int(i) => *i,
            other => return Err(conversion_error('c', other)),
        };
        let c = u32::try_from(code_point)
            .ok()
            .filter(|c| *c <= 0x10ffff)
            .ok_or_else(|| {
                err(
                    "java.util.IllegalFormatCodePointException",
                    format!("Code point = 0x{:x}", code_point as u32),
                )
            })?;
        let text = char::from_u32(c).unwrap_or('\u{fffd}').to_string();
        Ok(justify(s, self.case(s, text)))
    }

    /// Writes ASCII digits in the locale's digits, with grouping separators in the integer part when `,` is given.
    fn localize(&mut self, s: &Spec, int: &str, frac: Option<&str>) -> String {
        let n = self.numbers();
        let digit = |c: char| char::from_u32(n.zero as u32 + (c as u32 - '0' as u32)).unwrap_or(c);
        let mut out = String::new();
        let len = int.len();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && s.has(GROUP) && n.group_size > 0 && (len - i).is_multiple_of(n.group_size) {
                out.push(n.grouping);
            }
            out.push(digit(c));
        }
        if let Some(frac) = frac {
            out.push(n.decimal);
            out.extend(frac.chars().map(digit));
        }
        out
    }

    /// Adds the sign (`-`, `(…)`, `+`, ` `), then zero padding after it up to the width, then justifies.
    fn signed(&mut self, s: &Spec, negative: bool, body: String, zero_pad: bool) -> String {
        let (prefix, suffix) = if negative {
            if s.has(PAREN) { ("(", ")") } else { ("-", "") }
        } else if s.has(PLUS) {
            ("+", "")
        } else if s.has(SPACE) {
            (" ", "")
        } else {
            ("", "")
        };
        self.padded(s, prefix, body, suffix, zero_pad)
    }

    fn padded(
        &mut self,
        s: &Spec,
        prefix: &str,
        body: String,
        suffix: &str,
        zero_pad: bool,
    ) -> String {
        let mut body = body;
        if zero_pad && s.has(ZERO) {
            let zero = self.numbers().zero;
            let width = s.width.unwrap_or(0) as usize;
            let len = java_len(prefix) + java_len(&body) + java_len(suffix);
            if len < width {
                body = zero.to_string().repeat(width - len) + &body;
            }
        }
        justify(s, format!("{prefix}{body}{suffix}"))
    }

    fn integral(&mut self, s: &Spec, arg: &JValue) -> Result<String, JError> {
        // (signed value, bits of the two's complement form for `o`/`x`, `None` for BigInteger)
        let (value, bits): (BigInt, Option<u32>) = match arg {
            JValue::Byte(v) => ((*v).into(), Some(8)),
            JValue::Short(v) => ((*v).into(), Some(16)),
            JValue::Int(v) => ((*v).into(), Some(32)),
            JValue::Long(v) => ((*v).into(), Some(64)),
            JValue::BigInteger(text) => (
                text.parse()
                    .map_err(|_| conversion_error(s.conversion, arg))?,
                None,
            ),
            other => return Err(conversion_error(s.conversion, other)),
        };
        let negative = value.sign() == Sign::Minus;
        let magnitude = value.magnitude().clone();
        if s.conversion == 'd' {
            let body = self.localize(s, &magnitude.to_string(), None);
            return Ok(self.signed(s, negative, body, true));
        }
        let radix = if s.conversion == 'o' { 8 } else { 16 };
        let alt = match (s.has(ALT), radix) {
            (false, _) => "",
            (true, 8) => "0",
            (true, _) => "0x",
        };
        let text = match bits {
            Some(bits) => {
                if s.flags & (PLUS | SPACE | PAREN) != 0 {
                    return Err(mismatch(s.conversion, s.flags & (PLUS | SPACE | PAREN)));
                }
                let raw = match arg {
                    JValue::Byte(v) => *v as u8 as u64,
                    JValue::Short(v) => *v as u16 as u64,
                    JValue::Int(v) => *v as u32 as u64,
                    JValue::Long(v) => *v as u64,
                    _ => unreachable!(),
                };
                let _ = bits;
                let digits = if radix == 8 {
                    format!("{raw:o}")
                } else {
                    format!("{raw:x}")
                };
                let body = self.zero_fill(s, alt.len(), digits);
                justify(s, format!("{alt}{body}"))
            }
            None => {
                let digits = magnitude.to_str_radix(radix);
                let (prefix, suffix) = if negative {
                    if s.has(PAREN) { ("(", ")") } else { ("-", "") }
                } else if s.has(PLUS) {
                    ("+", "")
                } else if s.has(SPACE) {
                    (" ", "")
                } else {
                    ("", "")
                };
                let body = self.zero_fill(s, prefix.len() + alt.len() + suffix.len(), digits);
                justify(s, format!("{prefix}{alt}{body}{suffix}"))
            }
        };
        Ok(self.case(s, text))
    }

    /// ASCII zeros in front of `digits` so that `other + digits` reaches the width, when `0` is given.
    fn zero_fill(&self, s: &Spec, other: usize, digits: String) -> String {
        match s.width {
            Some(w) if s.has(ZERO) && other + digits.len() < w as usize => {
                "0".repeat(w as usize - other - digits.len()) + &digits
            }
            _ => digits,
        }
    }

    fn floating(&mut self, s: &Spec, arg: &JValue) -> Result<String, JError> {
        let value = match arg {
            JValue::Double(d) => Floating::Double(*d),
            JValue::Float(f) => Floating::Double(*f as f64),
            JValue::BigDecimal(text) if s.conversion != 'a' => Floating::Decimal(
                BigDec::parse(text).ok_or_else(|| conversion_error(s.conversion, arg))?,
            ),
            other => return Err(conversion_error(s.conversion, other)),
        };
        let (negative, decimal) = match value {
            Floating::Double(d) => {
                if d.is_nan() {
                    return Ok(justify(s, self.case(s, "NaN".into())));
                }
                if d.is_infinite() {
                    let negative = d < 0.0;
                    let text = self.signed_plain(s, negative, "Infinity");
                    return Ok(justify(s, self.case(s, text)));
                }
                if s.conversion == 'a' {
                    let body = hex_float(d.abs(), s.precision);
                    let text = self.hex_signed(s, d.is_sign_negative(), body);
                    return Ok(self.case(s, text));
                }
                (d.is_sign_negative(), Dec::from_f64(d.abs()))
            }
            Floating::Decimal(b) => (b.is_negative(), Dec::from_big_dec(&b.abs())),
        };
        let body = match s.conversion {
            'e' => self.scientific(s, decimal, s.precision.unwrap_or(6) as usize),
            'f' => self.fixed(s, decimal, s.precision.unwrap_or(6) as usize),
            'g' => {
                let p = match s.precision {
                    None => 6,
                    Some(0) => 1,
                    Some(p) => p as usize,
                };
                self.general(s, decimal, p)
            }
            _ => unreachable!(),
        };
        let text = self.signed(s, negative, body, true);
        Ok(self.case(s, text))
    }

    /// `Infinity` with its sign, never zero padded.
    fn signed_plain(&self, s: &Spec, negative: bool, text: &str) -> String {
        if negative {
            if s.has(PAREN) {
                format!("({text})")
            } else {
                format!("-{text}")
            }
        } else if s.has(PLUS) {
            format!("+{text}")
        } else if s.has(SPACE) {
            format!(" {text}")
        } else {
            text.to_string()
        }
    }

    /// `%a`: the sign, then `0x`, zero padding after it, the rest.
    fn hex_signed(&mut self, s: &Spec, negative: bool, body: String) -> String {
        let sign = if negative {
            "-"
        } else if s.has(PLUS) {
            "+"
        } else if s.has(SPACE) {
            " "
        } else {
            ""
        };
        // The JDK measures the zero padding against the value without the trailing zeros of its hex digits (as
        // `Double.toHexString` writes it), then prints the digits padded to the precision: recorded in the
        // fixtures (`%014.17a` of 1.0 is `0x00000001.00000000000000000p0`, wider than 14).
        let (mantissa, exponent) = body.split_once('p').expect("hex float");
        let (int, frac) = mantissa.split_once('.').expect("hex float");
        let trimmed = frac.trim_end_matches('0');
        let trimmed_len = int.len() + 1 + trimmed.len().max(1) + 1 + exponent.len();
        let mut zeros = String::new();
        if let Some(w) = s.width.filter(|_| s.has(ZERO)) {
            let len = sign.len() + trimmed_len;
            if len < w as usize {
                zeros = "0".repeat(w as usize - len);
            }
        }
        justify(s, format!("{sign}0x{zeros}{}", &body[2..]))
    }

    fn scientific(&mut self, s: &Spec, mut d: Dec, precision: usize) -> String {
        let exponent = if d.is_zero() {
            d.zero_exponent
        } else {
            d.round_to(precision as i64 + 1 - d.point);
            d.point - 1
        };
        let mut mantissa: Vec<u8> = d.digits.clone();
        mantissa.resize(precision + 1, b'0');
        let int = (mantissa[0] as char).to_string();
        let frac = String::from_utf8(mantissa[1..].to_vec()).expect("ascii");
        let mut body = if precision == 0 && !s.has(ALT) {
            self.localize(s, &int, None)
        } else {
            self.localize(s, &int, Some(&frac))
        };
        body.push('e');
        body.push(if exponent < 0 { '-' } else { '+' });
        let exp_digits = format!("{:02}", exponent.unsigned_abs());
        body.push_str(&self.localize(
            &Spec {
                flags: 0,
                ..s.clone()
            },
            &exp_digits,
            None,
        ));
        body
    }

    fn fixed(&mut self, s: &Spec, mut d: Dec, precision: usize) -> String {
        d.round_to(precision as i64);
        let (int, frac) = d.split(precision);
        if precision == 0 && !s.has(ALT) {
            self.localize(s, &int, None)
        } else {
            self.localize(s, &int, Some(&frac))
        }
    }

    fn general(&mut self, s: &Spec, mut d: Dec, precision: usize) -> String {
        if d.is_zero() {
            if d.zero_exponent == 0 {
                return self.fixed(s, d, precision - 1);
            }
            return self.scientific(s, d, precision - 1);
        }
        d.round_to(precision as i64 - d.point);
        // 10^-4 <= value < 10^precision
        if d.point >= -3 && d.point <= precision as i64 {
            let frac = (precision as i64 - d.point) as usize;
            self.fixed(s, d, frac)
        } else {
            self.scientific(s, d, precision - 1)
        }
    }

    fn date_time(&mut self, s: &Spec, arg: &JValue) -> Result<String, JError> {
        let conversion = s.date.expect("date conversion");
        let mut calendar = match arg {
            JValue::Long(millis) => {
                GregorianCalendar::new(*millis, &TimeZone::default_zone(), self.locale)
            }
            other => match super::objects::date_millis(other) {
                Some(millis) => {
                    GregorianCalendar::new(millis, &TimeZone::default_zone(), self.locale)
                }
                None => super::objects::calendar_of(other)
                    .ok_or_else(|| conversion_error(conversion, other))?,
            },
        };
        let text = self.date_field(&mut calendar, conversion)?;
        Ok(justify(s, self.case(s, text)))
    }

    /// A number in the locale's digits, at least `width` digits long.
    fn date_number(&mut self, value: i64, width: usize) -> String {
        let digits = format!("{:0width$}", value.unsigned_abs());
        let zero = self.numbers().zero;
        let local: String = digits
            .chars()
            .map(|c| char::from_u32(zero as u32 + (c as u32 - '0' as u32)).unwrap_or(c))
            .collect();
        if value < 0 {
            format!("-{local}")
        } else {
            local
        }
    }

    fn date_field(
        &mut self,
        c: &mut GregorianCalendar,
        conversion: char,
    ) -> Result<String, JError> {
        let symbols = || DateFormatSymbols::new(self.locale);
        Ok(match conversion {
            'H' => self.date_number(c.get(HOUR_OF_DAY) as i64, 2),
            'I' => self.date_number(twelve_hour(c) as i64, 2),
            'k' => self.date_number(c.get(HOUR_OF_DAY) as i64, 0),
            'l' => self.date_number(twelve_hour(c) as i64, 0),
            'M' => self.date_number(c.get(MINUTE) as i64, 2),
            'S' => self.date_number(c.get(SECOND) as i64, 2),
            'L' => self.date_number(millis_of_second(c) as i64, 3),
            'N' => self.date_number(millis_of_second(c) as i64 * 1_000_000, 9),
            'p' => to_lower(symbols().am_pm[c.get(AM_PM) as usize], self.locale),
            'z' => {
                let offset = (c.get(ZONE_OFFSET) + c.get(DST_OFFSET)) / 60_000;
                let sign = if offset < 0 { '-' } else { '+' };
                let offset = offset.abs();
                format!(
                    "{sign}{}",
                    self.date_number((offset / 60 * 100 + offset % 60) as i64, 4)
                )
            }
            'Z' => {
                let daylight = c.get(DST_OFFSET) != 0;
                c.time_zone()
                    .display_name(daylight, crate::util::timezone::SHORT, self.locale)
            }
            's' => self.date_number(c.time_in_millis() / 1000, 0),
            'Q' => self.date_number(c.time_in_millis(), 0),
            'B' => symbols().months[c.get(MONTH) as usize].to_string(),
            'b' | 'h' => symbols().short_months[c.get(MONTH) as usize].to_string(),
            'A' => symbols().weekdays[(c.get(DAY_OF_WEEK) - 1) as usize].to_string(),
            'a' => symbols().short_weekdays[(c.get(DAY_OF_WEEK) - 1) as usize].to_string(),
            'C' => self.date_number((c.get(YEAR) / 100) as i64, 2),
            'Y' => self.date_number(c.get(YEAR) as i64, 4),
            'y' => self.date_number((c.get(YEAR) % 100) as i64, 2),
            'j' => self.date_number(c.get(DAY_OF_YEAR) as i64, 3),
            'm' => self.date_number(c.get(MONTH) as i64 + 1, 2),
            'd' => self.date_number(c.get(DAY_OF_MONTH) as i64, 2),
            'e' => self.date_number(c.get(DAY_OF_MONTH) as i64, 0),
            'R' => self.date_fields(c, "HM", ":")?,
            'T' => self.date_fields(c, "HMS", ":")?,
            'r' => format!(
                "{} {}",
                self.date_fields(c, "IMS", ":")?,
                to_upper(&self.date_field(c, 'p')?, self.locale)
            ),
            'D' => self.date_fields(c, "mdy", "/")?,
            'F' => self.date_fields(c, "Ymd", "-")?,
            'c' => self.date_fields(c, "abdTZY", " ")?,
            other => unreachable!("date conversion {other}"),
        })
    }

    fn date_fields(
        &mut self,
        c: &mut GregorianCalendar,
        fields: &str,
        separator: &str,
    ) -> Result<String, JError> {
        let mut parts = Vec::new();
        for f in fields.chars() {
            parts.push(self.date_field(c, f)?);
        }
        Ok(parts.join(separator))
    }
}

fn twelve_hour(c: &mut GregorianCalendar) -> i32 {
    match c.get(HOUR) {
        0 => 12,
        h => h,
    }
}

fn millis_of_second(c: &mut GregorianCalendar) -> i32 {
    c.get(crate::util::calendar::MILLISECOND)
}

/// `Objects.hashCode` as `%h` prints it.
fn hash_code(v: &JValue) -> i32 {
    match super::objects::date_millis(v) {
        Some(millis) => crate::util::Date(millis).java_hash_code(),
        None => java_hash_code(v),
    }
}

enum Floating {
    Double(f64),
    Decimal(BigDec),
}

/// A non-negative decimal `0.d₁d₂… × 10^point` with ASCII `digits` (no leading zeros; empty for zero). A zero
/// keeps the exponent `%e` prints for it: `-scale` of a `BigDecimal`, 0 for a `double`.
#[derive(Debug, Clone)]
struct Dec {
    digits: Vec<u8>,
    point: i64,
    zero_exponent: i64,
}

impl Dec {
    fn new(digits: &[u8], point: i64, zero_exponent: i64) -> Dec {
        let lead = digits.iter().take_while(|d| **d == b'0').count();
        let digits = digits[lead..].to_vec();
        let point = if digits.is_empty() {
            0
        } else {
            point - lead as i64
        };
        Dec {
            digits,
            point,
            zero_exponent,
        }
    }

    /// The digits of `Double.toString(value)`.
    fn from_f64(value: f64) -> Dec {
        if value == 0.0 {
            return Dec::new(&[], 0, 0);
        }
        let text = double_to_string(value);
        let (mantissa, exponent) = match text.split_once('E') {
            Some((m, e)) => (m.to_string(), e.parse::<i64>().expect("exponent")),
            None => (text.clone(), 0),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((&mantissa, ""));
        let digits: Vec<u8> = format!("{int}{frac}").into_bytes();
        Dec::new(&digits, int.len() as i64 + exponent, 0)
    }

    fn from_big_dec(value: &BigDec) -> Dec {
        let text = value.unscaled.magnitude().to_string();
        Dec::new(
            text.as_bytes(),
            text.len() as i64 - value.scale,
            -value.scale,
        )
    }

    fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// Rounds `HALF_UP` to `places` digits after the decimal point (negative: to tens, hundreds, …).
    fn round_to(&mut self, places: i64) {
        let keep = self.point + places;
        if keep >= self.digits.len() as i64 {
            return;
        }
        if keep < 0 {
            *self = Dec::new(&[], 0, self.zero_exponent);
            return;
        }
        let keep = keep as usize;
        let up = self.digits[keep] >= b'5';
        let mut kept = self.digits[..keep].to_vec();
        let mut point = self.point;
        if up {
            let mut i = kept.len();
            loop {
                if i == 0 {
                    kept.insert(0, b'1');
                    point += 1;
                    break;
                }
                i -= 1;
                if kept[i] == b'9' {
                    kept[i] = b'0';
                } else {
                    kept[i] += 1;
                    break;
                }
            }
        }
        let len = kept.len() - kept.iter().rev().take_while(|d| **d == b'0').count();
        kept.truncate(len);
        *self = Dec::new(&kept, point, self.zero_exponent);
        if self.is_zero() {
            self.point = 0;
        }
    }

    /// Integer digits (at least `0`) and exactly `places` fraction digits.
    fn split(&self, places: usize) -> (String, String) {
        let digit = |i: i64| -> char {
            if i >= 0 && (i as usize) < self.digits.len() {
                self.digits[i as usize] as char
            } else {
                '0'
            }
        };
        let int: String = if self.point > 0 {
            (0..self.point).map(digit).collect()
        } else {
            "0".into()
        };
        let frac: String = (0..places as i64).map(|k| digit(self.point + k)).collect();
        (int, frac)
    }
}

/// `%a` of a finite non-negative double: `Double.toHexString` (padded with zeros to a precision of 13 or more),
/// or for a smaller precision the normalised significand rounded half-even to that many hex digits.
fn hex_float(d: f64, precision: Option<i32>) -> String {
    let precision = precision.map(|p| p.max(1) as usize);
    if d == 0.0 {
        let zeros = "0".repeat(precision.unwrap_or(1));
        return format!("0x0.{zeros}p0");
    }
    let bits = d.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let fraction = bits & ((1u64 << 52) - 1);
    match precision {
        Some(p) if p < 13 => {
            let (mut significand, mut exponent) = if biased == 0 {
                let mut m = fraction;
                let mut e = -1022i64;
                while m & (1u64 << 52) == 0 {
                    m <<= 1;
                    e -= 1;
                }
                (m, e)
            } else {
                (fraction | (1u64 << 52), biased - 1023)
            };
            let drop = 52 - 4 * p as u32;
            let mut q = significand >> drop;
            let rest = significand & ((1u64 << drop) - 1);
            let half = 1u64 << (drop - 1);
            if rest > half || (rest == half && q & 1 == 1) {
                q += 1;
            }
            if q >> (4 * p as u32) >= 2 {
                q >>= 1;
                exponent += 1;
            }
            significand = q & ((1u64 << (4 * p as u32)) - 1);
            format!("0x1.{:0w$x}p{exponent}", significand, w = p)
        }
        _ => {
            let (lead, exponent) = if biased == 0 {
                ('0', -1022)
            } else {
                ('1', biased - 1023)
            };
            let mut hex = format!("{fraction:013x}");
            match precision {
                None => {
                    let len = hex.trim_end_matches('0').len().max(1);
                    hex.truncate(len);
                }
                Some(p) => hex.push_str(&"0".repeat(p - 13)),
            }
            format!("0x{lead}.{hex}p{exponent}")
        }
    }
}
