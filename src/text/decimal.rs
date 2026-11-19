//! `java.text.DecimalFormat` and the `NumberFormat` factories: the pattern grammar (`applyPattern`,
//! `applyLocalizedPattern`, `toPattern`), the getters and setters, formatting of every numeric [`JValue`] and
//! parsing with `ParsePosition`.
//!
//! Written from the Java SE API specification (`DecimalFormat`, `NumberFormat`, `ParsePosition`) and from
//! behaviour recorded in real JDKs: the fixtures under `tests/fixtures/` and jshell runs on JDK 21 for the
//! corners the specification leaves open (see `docs/provenance.md`). Notable recorded behaviour:
//!
//! - a number character met inside a suffix continues the number (`0x0` is `00` with suffix `x`);
//! - unquoted exponent symbols right after the exponent digits stay in the positive suffix but are left out of
//!   the negative one, whose implicit minus sign is then dropped too (`0E0E` formats -1 as `1E0`);
//! - with an exponent, the mantissa keeps up to `maximumIntegerDigits + maximumFractionDigits` significant digits;
//! - `double` and `long` values use at most 309 integer and 340 fraction digits, `BigInteger`/`BigDecimal` all.

use num_bigint::BigInt;
use num_traits::Zero;

use super::digits::{BigDec, Decimal, RoundingMode};
use super::locale::Locale;
use super::symbols::{DecimalFormatSymbols, number_patterns};
use crate::lang::{JError, JValue};

/// `java.text.ParsePosition`: where parsing starts and, afterwards, where it stopped (indices in UTF-16 units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsePosition {
    /// `getIndex()`.
    pub index: usize,
    /// `-1` until a parse fails.
    pub error_index: i64,
}

impl ParsePosition {
    /// `new ParsePosition(index)`.
    pub fn new(index: usize) -> ParsePosition {
        ParsePosition { index, error_index: -1 }
    }
}

/// Integer digits used for values other than `BigInteger` and `BigDecimal` (`setMaximumIntegerDigits`).
const DOUBLE_INTEGER_DIGITS: i32 = 309;
/// Fraction digits used for values other than `BigInteger` and `BigDecimal`.
const DOUBLE_FRACTION_DIGITS: i32 = 340;

/// One element of an affix as a pattern wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Text(String),
    Minus,
    Percent,
    PerMille,
    Currency,
    IntlCurrency,
}

/// A prefix or suffix: its pattern form (absent once a setter assigned literal text) and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Affix {
    tokens: Option<Vec<Token>>,
    text: String,
}

impl Affix {
    fn literal(text: &str) -> Affix {
        Affix { tokens: None, text: text.to_string() }
    }

    fn from_tokens(tokens: Vec<Token>, symbols: &DecimalFormatSymbols) -> Affix {
        let mut a = Affix { tokens: Some(tokens), text: String::new() };
        a.expand(symbols);
        a
    }

    /// Recomputes the text of a pattern affix from the symbols.
    fn expand(&mut self, symbols: &DecimalFormatSymbols) {
        if let Some(tokens) = &self.tokens {
            self.text = tokens
                .iter()
                .map(|t| match t {
                    Token::Text(s) => s.clone(),
                    Token::Minus => symbols.minus_text(),
                    Token::Percent => symbols.percent_text(),
                    Token::PerMille => symbols.per_mill_text(),
                    Token::Currency => symbols.currency_symbol.clone(),
                    Token::IntlCurrency => symbols.international_currency_symbol.clone(),
                })
                .collect();
        }
    }
}

/// Appends `c` to the token list, merging adjacent text.
fn push_text(tokens: &mut Vec<Token>, s: &str) {
    if let Some(Token::Text(t)) = tokens.last_mut() {
        t.push_str(s);
    } else {
        tokens.push(Token::Text(s.to_string()));
    }
}

/// The characters a pattern is written with: the ASCII set, or the symbols' for localized patterns.
struct Syntax {
    zero: char,
    digit: char,
    grouping: char,
    decimal: char,
    percent: char,
    per_mille: char,
    minus: char,
    separator: char,
    exponent: String,
}

const QUOTE: char = '\'';
const CURRENCY: char = '\u{a4}';

impl Syntax {
    fn new(localized: bool, s: &DecimalFormatSymbols) -> Syntax {
        if localized {
            Syntax {
                zero: s.zero_digit,
                digit: s.digit,
                grouping: s.grouping_separator,
                decimal: s.decimal_separator,
                percent: s.percent,
                per_mille: s.per_mill,
                minus: s.minus_sign,
                separator: s.pattern_separator,
                exponent: s.exponent_separator.clone(),
            }
        } else {
            Syntax {
                zero: '0',
                digit: '#',
                grouping: ',',
                decimal: '.',
                percent: '%',
                per_mille: '\u{2030}',
                minus: '-',
                separator: ';',
                exponent: "E".into(),
            }
        }
    }

    fn is_number_char(&self, c: char) -> bool {
        c == self.zero || c == self.digit || c == self.grouping || c == self.decimal
    }

    /// Literal affix text as a pattern writes it: quotes doubled, the whole text quoted when it holds a character
    /// that is special in affixes or numbers.
    fn quote(&self, text: &str) -> String {
        let special = [self.zero, self.digit, self.grouping, self.decimal, self.percent, self.per_mille, self.minus, self.separator, CURRENCY];
        let escaped = text.replace(QUOTE, "''");
        if text.chars().any(|c| special.contains(&c)) { format!("'{escaped}'") } else { escaped }
    }
}

/// What a pattern says about the number, before the implied-digit adjustments.
#[derive(Debug, Default)]
struct Shape {
    hashes_left: i32,
    zeros_left: i32,
    zeros_right: i32,
    hashes_right: i32,
    decimal: bool,
    /// Integer digits since the last grouping separator, once one was seen.
    group: Option<i32>,
    exponent_digits: Option<i32>,
}

/// One subpattern's affixes.
#[derive(Debug, Default)]
struct Sub {
    prefix: Vec<Token>,
    suffix: Vec<Token>,
    percents: u32,
    /// Unquoted exponent symbols directly after the exponent digits.
    exponent_run: usize,
}

/// The result of reading a pattern.
struct Compiled {
    shape: Shape,
    positive: Sub,
    negative: Option<Sub>,
}

fn malformed(kind: &str, pattern: &str) -> JError {
    JError::new("java.lang.IllegalArgumentException", format!("{kind} \"{pattern}\""))
}

fn compile(pattern: &str, sx: &Syntax) -> Result<Compiled, JError> {
    #[derive(PartialEq, Clone, Copy)]
    enum Phase {
        Prefix,
        Number,
        Suffix,
    }
    let chars: Vec<char> = pattern.chars().collect();
    let exp: Vec<char> = sx.exponent.chars().collect();
    let exp_at = |i: usize| !exp.is_empty() && chars.len() >= i + exp.len() && chars[i..i + exp.len()] == exp[..];
    let mut shape = Shape::default();
    let mut subs: Vec<Sub> = Vec::new();
    let mut i = 0;
    for part in 0..2 {
        if part == 1 && i >= chars.len() {
            break;
        }
        let mut sub = Sub::default();
        let mut phase = Phase::Prefix;
        while i < chars.len() {
            let c = chars[i];
            if phase == Phase::Number {
                if part == 1 {
                    // The negative subpattern's number is skipped; only its affixes count.
                    if sx.is_number_char(c) {
                        i += 1;
                        continue;
                    }
                    if exp_at(i) {
                        i += exp.len();
                        while i < chars.len() && chars[i] == sx.zero {
                            i += 1;
                        }
                        while exp_at(i) {
                            sub.exponent_run += 1;
                            push_text(&mut sub.suffix, &sx.exponent);
                            i += exp.len();
                        }
                    }
                    phase = Phase::Suffix;
                    continue;
                }
                if c == sx.digit {
                    if shape.decimal {
                        shape.hashes_right += 1;
                    } else {
                        if shape.zeros_left > 0 {
                            return Err(malformed("Malformed pattern", pattern));
                        }
                        shape.hashes_left += 1;
                        shape.group = shape.group.map(|g| g + 1);
                    }
                } else if c == sx.zero {
                    if shape.decimal {
                        if shape.hashes_right > 0 {
                            return Err(malformed("Unexpected '0' in pattern", pattern));
                        }
                        shape.zeros_right += 1;
                    } else {
                        shape.zeros_left += 1;
                        shape.group = shape.group.map(|g| g + 1);
                    }
                } else if c == sx.grouping {
                    if shape.decimal {
                        return Err(malformed("Malformed pattern", pattern));
                    }
                    shape.group = Some(0);
                } else if c == sx.decimal {
                    if shape.decimal {
                        return Err(malformed("Multiple decimal separators in pattern", pattern));
                    }
                    if shape.group == Some(0) {
                        return Err(malformed("Malformed pattern", pattern));
                    }
                    shape.decimal = true;
                } else if exp_at(i) {
                    if shape.exponent_digits.is_some() {
                        return Err(malformed("Multiple exponential symbols in pattern", pattern));
                    }
                    if shape.group == Some(0) {
                        return Err(malformed("Malformed pattern", pattern));
                    }
                    i += exp.len();
                    let mut n = 0;
                    while i < chars.len() && chars[i] == sx.zero {
                        n += 1;
                        i += 1;
                    }
                    if n == 0 {
                        return Err(malformed("Malformed exponential pattern", pattern));
                    }
                    shape.exponent_digits = Some(n);
                    while exp_at(i) {
                        sub.exponent_run += 1;
                        push_text(&mut sub.suffix, &sx.exponent);
                        i += exp.len();
                    }
                    phase = Phase::Suffix;
                    continue;
                } else {
                    phase = Phase::Suffix;
                    continue;
                }
                i += 1;
                continue;
            }
            // Prefix or suffix.
            if c == sx.percent || c == sx.per_mille {
                sub.percents += 1;
                if sub.percents > 1 {
                    return Err(malformed("Too many percent/per mille characters in pattern", pattern));
                }
            }
            let affix = if phase == Phase::Prefix { &mut sub.prefix } else { &mut sub.suffix };
            if c == QUOTE {
                if chars.get(i + 1) == Some(&QUOTE) {
                    push_text(affix, "'");
                    i += 2;
                    continue;
                }
                let mut text = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(malformed("Malformed pattern", pattern)),
                        Some(&QUOTE) if chars.get(i + 1) == Some(&QUOTE) => {
                            text.push(QUOTE);
                            i += 2;
                        }
                        Some(&QUOTE) => {
                            i += 1;
                            break;
                        }
                        Some(&ch) => {
                            text.push(ch);
                            i += 1;
                        }
                    }
                }
                push_text(affix, &text);
                continue;
            }
            if sx.is_number_char(c) {
                phase = Phase::Number;
                continue;
            }
            if c == sx.separator {
                if phase == Phase::Prefix || part == 1 {
                    return Err(malformed("Unquoted special character ';' in pattern", pattern));
                }
                i += 1;
                break;
            }
            if c == sx.percent || c == sx.per_mille {
                affix.push(if c == sx.percent { Token::Percent } else { Token::PerMille });
            } else if c == CURRENCY {
                if chars.get(i + 1) == Some(&CURRENCY) {
                    affix.push(Token::IntlCurrency);
                    i += 1;
                } else {
                    affix.push(Token::Currency);
                }
            } else if c == sx.minus {
                affix.push(Token::Minus);
            } else {
                push_text(affix, &c.to_string());
            }
            i += 1;
        }
        subs.push(sub);
    }
    if shape.group == Some(0) {
        return Err(malformed("Malformed pattern", pattern));
    }
    let mut subs = subs.into_iter();
    let positive = subs.next().unwrap_or_default();
    Ok(Compiled { shape, positive, negative: subs.next() })
}

/// Drops `run` leading exponent symbols from a suffix.
fn without_exponent_run(mut suffix: Vec<Token>, run: usize, exponent: &str) -> Vec<Token> {
    if run > 0 {
        if let Some(Token::Text(t)) = suffix.first_mut() {
            let cut = exponent.repeat(run);
            if let Some(rest) = t.strip_prefix(&cut) {
                *t = rest.to_string();
                if t.is_empty() {
                    suffix.remove(0);
                }
            }
        }
    }
    suffix
}

/// `java.text.DecimalFormat`.
#[derive(Debug, Clone)]
pub struct DecimalFormat {
    symbols: DecimalFormatSymbols,
    positive_prefix: Affix,
    positive_suffix: Affix,
    negative_prefix: Affix,
    negative_suffix: Affix,
    multiplier: i32,
    grouping_size: i32,
    grouping_used: bool,
    decimal_separator_always_shown: bool,
    /// Minimum exponent digits of a scientific pattern.
    exponent_digits: Option<i32>,
    /// Whether the pattern has a currency sign (monetary separators are used).
    currency: bool,
    min_int: i32,
    max_int: i32,
    min_frac: i32,
    max_frac: i32,
    rounding: RoundingMode,
    parse_big_decimal: bool,
    parse_integer_only: bool,
}

impl DecimalFormat {
    /// `new DecimalFormat(pattern, symbols)`.
    pub fn new(pattern: &str, symbols: &DecimalFormatSymbols) -> Result<DecimalFormat, JError> {
        let mut f = DecimalFormat {
            symbols: symbols.clone(),
            positive_prefix: Affix::literal(""),
            positive_suffix: Affix::literal(""),
            negative_prefix: Affix::literal("-"),
            negative_suffix: Affix::literal(""),
            multiplier: 1,
            grouping_size: 3,
            grouping_used: true,
            decimal_separator_always_shown: false,
            exponent_digits: None,
            currency: false,
            min_int: 1,
            max_int: i32::MAX,
            min_frac: 0,
            max_frac: 3,
            rounding: RoundingMode::HalfEven,
            parse_big_decimal: false,
            parse_integer_only: false,
        };
        f.load_pattern(pattern, false)?;
        Ok(f)
    }

    /// `new DecimalFormat(pattern, DecimalFormatSymbols.getInstance(locale))`.
    pub fn with_locale(pattern: &str, locale: &Locale) -> Result<DecimalFormat, JError> {
        DecimalFormat::new(pattern, &DecimalFormatSymbols::new(locale))
    }

    /// `getDecimalFormatSymbols()`.
    pub fn symbols(&self) -> &DecimalFormatSymbols {
        &self.symbols
    }

    /// `setDecimalFormatSymbols`: affixes written in the pattern follow the new symbols.
    pub fn set_decimal_format_symbols(&mut self, symbols: &DecimalFormatSymbols) {
        self.symbols = symbols.clone();
        for a in [&mut self.positive_prefix, &mut self.positive_suffix, &mut self.negative_prefix, &mut self.negative_suffix] {
            a.expand(symbols);
        }
    }

    /// `applyPattern(pattern)`.
    pub fn apply_pattern(&mut self, pattern: &str) -> Result<(), JError> {
        self.load_pattern(pattern, false)
    }

    /// `applyLocalizedPattern(pattern)`.
    pub fn apply_localized_pattern(&mut self, pattern: &str) -> Result<(), JError> {
        self.load_pattern(pattern, true)
    }

    fn load_pattern(&mut self, pattern: &str, localized: bool) -> Result<(), JError> {
        let sx = Syntax::new(localized, &self.symbols);
        if pattern.is_empty() {
            // Recorded from JDK 26: an empty pattern formats with every fraction digit a double has.
            self.set_affixes_from(vec![], vec![], None, 0, &sx);
            self.multiplier = 1;
            self.grouping_size = 3;
            self.grouping_used = true;
            self.decimal_separator_always_shown = false;
            self.exponent_digits = None;
            self.currency = false;
            self.min_int = 0;
            self.max_int = i32::MAX;
            self.min_frac = 0;
            self.max_frac = DOUBLE_FRACTION_DIGITS;
            return Ok(());
        }
        let Compiled { shape, positive, negative } = compile(pattern, &sx)?;
        let mut hl = shape.hashes_left;
        let mut zl = shape.zeros_left;
        let mut hr = shape.hashes_right;
        let mut zr = shape.zeros_right;
        // A pattern without any '0' still shows one digit next to its decimal point: "#.##" is "#0.##",
        // ".###" is ".0##".
        if zl == 0 && shape.decimal {
            if hl > 0 {
                hl -= 1;
                zl = 1;
            } else if zr == 0 && hr > 0 {
                hr -= 1;
                zr = 1;
            }
        }
        let has_currency = |s: &Sub| s.prefix.iter().chain(&s.suffix).any(|t| matches!(t, Token::Currency | Token::IntlCurrency));
        self.currency = has_currency(&positive) || negative.as_ref().is_some_and(has_currency);
        self.multiplier = if positive.prefix.iter().chain(&positive.suffix).any(|t| *t == Token::PerMille) {
            1000
        } else if positive.prefix.iter().chain(&positive.suffix).any(|t| *t == Token::Percent) {
            100
        } else {
            1
        };
        self.grouping_size = shape.group.unwrap_or(0);
        self.grouping_used = shape.group.is_some();
        self.decimal_separator_always_shown = shape.decimal && (hl + zl == 0 || hr + zr == 0);
        self.exponent_digits = shape.exponent_digits;
        self.min_int = zl;
        self.max_int = if shape.exponent_digits.is_some() { hl + zl } else { i32::MAX };
        self.min_frac = zr;
        self.max_frac = zr + hr;
        let run = positive.exponent_run;
        self.set_affixes_from(positive.prefix, positive.suffix, negative, run, &sx);
        Ok(())
    }

    fn set_affixes_from(&mut self, prefix: Vec<Token>, suffix: Vec<Token>, negative: Option<Sub>, run: usize, sx: &Syntax) {
        // A negative subpattern without affixes counts as absent.
        let negative = negative.map(|n| {
            let r = n.exponent_run;
            (n.prefix, without_exponent_run(n.suffix, r, &sx.exponent))
        });
        let negative = negative.filter(|(p, s)| !(p.is_empty() && s.is_empty()));
        let (neg_prefix, neg_suffix) = match negative {
            Some(n) => n,
            None if run > 0 => (prefix.clone(), without_exponent_run(suffix.clone(), run, &sx.exponent)),
            None => {
                let mut p = vec![Token::Minus];
                p.extend(prefix.iter().cloned());
                (p, suffix.clone())
            }
        };
        let s = &self.symbols;
        self.positive_prefix = Affix::from_tokens(prefix, s);
        self.positive_suffix = Affix::from_tokens(suffix, s);
        self.negative_prefix = Affix::from_tokens(neg_prefix, s);
        self.negative_suffix = Affix::from_tokens(neg_suffix, s);
    }

    /// `toPattern()`.
    pub fn to_pattern(&self) -> String {
        self.write_pattern(false)
    }

    /// `toLocalizedPattern()`.
    pub fn to_localized_pattern(&self) -> String {
        self.write_pattern(true)
    }

    fn write_pattern(&self, localized: bool) -> String {
        let sx = Syntax::new(localized, &self.symbols);
        let number = self.number_pattern(&sx);
        let affix = |a: &Affix| match &a.tokens {
            Some(tokens) => tokens
                .iter()
                .map(|t| match t {
                    Token::Text(s) => sx.quote(s),
                    Token::Minus => sx.minus.to_string(),
                    Token::Percent => sx.percent.to_string(),
                    Token::PerMille => sx.per_mille.to_string(),
                    Token::Currency => CURRENCY.to_string(),
                    Token::IntlCurrency => format!("{CURRENCY}{CURRENCY}"),
                })
                .collect::<String>(),
            None => sx.quote(&a.text),
        };
        let mut out = format!("{}{number}{}", affix(&self.positive_prefix), affix(&self.positive_suffix));
        if !self.negative_is_implicit() {
            out.push(sx.separator);
            out.push_str(&format!("{}{number}{}", affix(&self.negative_prefix), affix(&self.negative_suffix)));
        }
        out
    }

    /// Whether the negative affixes are the ones a pattern without negative subpattern implies.
    fn negative_is_implicit(&self) -> bool {
        match (&self.positive_prefix.tokens, &self.positive_suffix.tokens, &self.negative_prefix.tokens, &self.negative_suffix.tokens) {
            (Some(pp), Some(ps), Some(np), Some(ns)) => np.first() == Some(&Token::Minus) && np[1..] == pp[..] && ns == ps,
            _ => {
                self.negative_prefix.text == format!("{}{}", self.symbols.minus_text(), self.positive_prefix.text)
                    && self.negative_suffix.text == self.positive_suffix.text
            }
        }
    }

    fn number_pattern(&self, sx: &Syntax) -> String {
        let mut s = String::new();
        let count = if self.exponent_digits.is_some() { self.max_int as i64 } else { self.min_int.max(self.grouping_size) as i64 + 1 };
        let grouping = self.grouping_used && self.grouping_size > 0;
        for k in (0..count).rev() {
            s.push(if k < self.min_int as i64 { sx.zero } else { sx.digit });
            if grouping && k > 0 && k % self.grouping_size as i64 == 0 {
                s.push(sx.grouping);
            }
        }
        if self.max_frac > 0 || self.decimal_separator_always_shown {
            s.push(sx.decimal);
        }
        for k in 0..self.max_frac {
            s.push(if k < self.min_frac { sx.zero } else { sx.digit });
        }
        if let Some(n) = self.exponent_digits {
            s.push_str(&sx.exponent);
            for _ in 0..n {
                s.push(sx.zero);
            }
        }
        s
    }

    /// `getPositivePrefix()`.
    pub fn positive_prefix(&self) -> &str {
        &self.positive_prefix.text
    }
    /// `getPositiveSuffix()`.
    pub fn positive_suffix(&self) -> &str {
        &self.positive_suffix.text
    }
    /// `getNegativePrefix()`.
    pub fn negative_prefix(&self) -> &str {
        &self.negative_prefix.text
    }
    /// `getNegativeSuffix()`.
    pub fn negative_suffix(&self) -> &str {
        &self.negative_suffix.text
    }
    /// `setPositivePrefix`.
    pub fn set_positive_prefix(&mut self, v: &str) {
        self.positive_prefix = Affix::literal(v);
    }
    /// `setPositiveSuffix`.
    pub fn set_positive_suffix(&mut self, v: &str) {
        self.positive_suffix = Affix::literal(v);
    }
    /// `setNegativePrefix`.
    pub fn set_negative_prefix(&mut self, v: &str) {
        self.negative_prefix = Affix::literal(v);
    }
    /// `setNegativeSuffix`.
    pub fn set_negative_suffix(&mut self, v: &str) {
        self.negative_suffix = Affix::literal(v);
    }
    /// `getMultiplier()`.
    pub fn multiplier(&self) -> i32 {
        self.multiplier
    }
    /// `setMultiplier`.
    pub fn set_multiplier(&mut self, v: i32) {
        self.multiplier = v;
    }
    /// `getGroupingSize()`.
    pub fn grouping_size(&self) -> i32 {
        self.grouping_size
    }
    /// `setGroupingSize`: 0 to 127.
    pub fn set_grouping_size(&mut self, v: i32) -> Result<(), JError> {
        if !(0..=127).contains(&v) {
            return Err(JError::new("java.lang.IllegalArgumentException", format!("newValue is out of valid range. value: {v}")));
        }
        self.grouping_size = v;
        Ok(())
    }
    /// `isGroupingUsed()`.
    pub fn is_grouping_used(&self) -> bool {
        self.grouping_used
    }
    /// `setGroupingUsed`.
    pub fn set_grouping_used(&mut self, v: bool) {
        self.grouping_used = v;
    }
    /// `isDecimalSeparatorAlwaysShown()`.
    pub fn is_decimal_separator_always_shown(&self) -> bool {
        self.decimal_separator_always_shown
    }
    /// `setDecimalSeparatorAlwaysShown`.
    pub fn set_decimal_separator_always_shown(&mut self, v: bool) {
        self.decimal_separator_always_shown = v;
    }
    /// `isParseBigDecimal()`.
    pub fn is_parse_big_decimal(&self) -> bool {
        self.parse_big_decimal
    }
    /// `setParseBigDecimal`.
    pub fn set_parse_big_decimal(&mut self, v: bool) {
        self.parse_big_decimal = v;
    }
    /// `isParseIntegerOnly()`.
    pub fn is_parse_integer_only(&self) -> bool {
        self.parse_integer_only
    }
    /// `setParseIntegerOnly`.
    pub fn set_parse_integer_only(&mut self, v: bool) {
        self.parse_integer_only = v;
    }
    /// `getRoundingMode()`.
    pub fn rounding_mode(&self) -> RoundingMode {
        self.rounding
    }
    /// `setRoundingMode`.
    pub fn set_rounding_mode(&mut self, v: RoundingMode) {
        self.rounding = v;
    }
    /// `getMaximumIntegerDigits()`.
    pub fn maximum_integer_digits(&self) -> i32 {
        self.max_int
    }
    /// `getMinimumIntegerDigits()`.
    pub fn minimum_integer_digits(&self) -> i32 {
        self.min_int
    }
    /// `getMaximumFractionDigits()`.
    pub fn maximum_fraction_digits(&self) -> i32 {
        self.max_frac
    }
    /// `getMinimumFractionDigits()`.
    pub fn minimum_fraction_digits(&self) -> i32 {
        self.min_frac
    }

    /// `setMaximumIntegerDigits`: negative counts as 0; the minimum follows a lower maximum.
    pub fn set_maximum_integer_digits(&mut self, v: i32) {
        self.max_int = v.max(0);
        self.min_int = self.min_int.min(self.max_int);
    }

    /// `setMinimumIntegerDigits`: negative counts as 0; the maximum follows a higher minimum.
    pub fn set_minimum_integer_digits(&mut self, v: i32) {
        self.min_int = v.max(0);
        self.max_int = self.max_int.max(self.min_int);
    }

    /// `setMaximumFractionDigits`.
    pub fn set_maximum_fraction_digits(&mut self, v: i32) {
        self.max_frac = v.max(0);
        self.min_frac = self.min_frac.min(self.max_frac);
    }

    /// `setMinimumFractionDigits`.
    pub fn set_minimum_fraction_digits(&mut self, v: i32) {
        self.min_frac = v.max(0);
        self.max_frac = self.max_frac.max(self.min_frac);
    }

    // ---------------------------------------------------------------------------------------------------------
    // Formatting

    /// `format(Object)`: any `Number` (byte, short, int, long, float, double, `BigInteger`, `BigDecimal`).
    pub fn format(&self, value: &JValue) -> Result<String, JError> {
        match value {
            JValue::Byte(b) => self.format_integer(*b as i128),
            JValue::Short(s) => self.format_integer(*s as i128),
            JValue::Int(i) => self.format_integer(*i as i128),
            JValue::Long(l) => self.format_integer(*l as i128),
            JValue::Float(f) => self.format_f64(*f as f64),
            JValue::Double(d) => self.format_f64(*d),
            JValue::BigInteger(s) => match s.parse::<BigInt>() {
                // A BigInteger within the range of long is formatted as a long (309-digit limits included).
                Ok(b) if i64::try_from(&b).is_ok() => self.format_integer(i64::try_from(&b).unwrap_or(0) as i128),
                Ok(b) => self.format_big(BigDec { unscaled: b, scale: 0 }),
                Err(_) => Err(not_a_number()),
            },
            JValue::BigDecimal(s) => BigDec::parse(s).map_or_else(|| Err(not_a_number()), |b| self.format_big(b)),
            _ => Err(not_a_number()),
        }
    }

    /// `format(double)`.
    pub fn format_f64(&self, d: f64) -> Result<String, JError> {
        if d.is_nan() {
            return Ok(self.symbols.nan.clone());
        }
        let d = if self.multiplier == 1 { d } else { d * self.multiplier as f64 };
        if d.is_nan() {
            return Ok(self.symbols.nan.clone());
        }
        let negative = d.is_sign_negative();
        if d.is_infinite() {
            return Ok(self.wrap(negative, &self.symbols.infinity));
        }
        self.render(Decimal::from_f64(d), negative, false)
    }

    /// `format(long)`.
    pub fn format_i64(&self, l: i64) -> Result<String, JError> {
        self.format_integer(l as i128)
    }

    fn format_integer(&self, n: i128) -> Result<String, JError> {
        let n = n * self.multiplier as i128;
        self.render(Decimal::from_integer_text(&n.unsigned_abs().to_string()), n < 0, false)
    }

    fn format_big(&self, b: BigDec) -> Result<String, JError> {
        let b = if self.multiplier == 1 { b } else { b.multiply_int(self.multiplier as i64) };
        self.render(Decimal::from_big_dec(&b), b.is_negative(), true)
    }

    fn wrap(&self, negative: bool, body: &str) -> String {
        if negative {
            format!("{}{body}{}", self.negative_prefix.text, self.negative_suffix.text)
        } else {
            format!("{}{body}{}", self.positive_prefix.text, self.positive_suffix.text)
        }
    }

    /// The digit limits for a value: `BigInteger`/`BigDecimal` use the settings as they are, other numbers at
    /// most 309 integer and 340 fraction digits.
    fn limits(&self, big: bool) -> (i64, i64, i64, i64) {
        let (mi, xi, mf, xf) = (self.min_int, self.max_int, self.min_frac, self.max_frac);
        if big {
            (mi as i64, xi as i64, mf as i64, xf as i64)
        } else {
            (
                mi.min(DOUBLE_INTEGER_DIGITS) as i64,
                xi.min(DOUBLE_INTEGER_DIGITS) as i64,
                mf.min(DOUBLE_FRACTION_DIGITS) as i64,
                xf.min(DOUBLE_FRACTION_DIGITS) as i64,
            )
        }
    }

    fn render(&self, mut value: Decimal, negative: bool, big: bool) -> Result<String, JError> {
        let (min_int, max_int, min_frac, max_frac) = self.limits(big);
        let (int_digits, frac_digits, exponent) = match self.exponent_digits {
            None => {
                value.round(value.point + max_frac, self.rounding, negative)?;
                let mut int = value.integer_digits();
                if int.len() as i64 > max_int {
                    int.drain(..int.len() - max_int as usize);
                }
                let mut frac = value.fraction_digits();
                frac.resize((frac.len() as i64).max(min_frac) as usize, b'0');
                if (int.len() as i64) < min_int {
                    let mut padded = vec![b'0'; (min_int - int.len() as i64) as usize];
                    padded.extend(int);
                    int = padded;
                }
                if int.is_empty() && frac.is_empty() {
                    int.push(b'0');
                }
                (int, frac, None)
            }
            Some(_) => {
                let engineering = max_int > min_int && max_int > 1;
                let min_sig = if engineering { 1 } else { min_int } + min_frac;
                let (int_count, exponent) = if value.is_zero() {
                    (if engineering { 1 } else { min_int }, 0)
                } else {
                    value.round((max_int + max_frac).max(1), self.rounding, negative)?;
                    let sci = value.point - 1;
                    if engineering {
                        let e = sci.div_euclid(max_int) * max_int;
                        (sci - e + 1, e)
                    } else {
                        (min_int, value.point - min_int)
                    }
                };
                let mut all = value.digits.clone();
                let total = (all.len() as i64).max(int_count).max(min_sig) as usize;
                all.resize(total, b'0');
                let frac = all.split_off(int_count as usize);
                (all, frac, Some(exponent))
            }
        };
        Ok(self.wrap(negative, &self.body(&int_digits, &frac_digits, exponent)))
    }

    /// The localized digits, separators and exponent of a rounded number.
    fn body(&self, int: &[u8], frac: &[u8], exponent: Option<i64>) -> String {
        let s = &self.symbols;
        let digit = |d: u8| char::from_u32(s.zero_digit as u32 + (d - b'0') as u32).unwrap_or('?');
        let (decimal, grouping) =
            if self.currency { (s.monetary_decimal_separator, s.monetary_grouping_separator) } else { (s.decimal_separator, s.grouping_separator) };
        let mut out = String::new();
        let group = exponent.is_none() && self.grouping_used && self.grouping_size > 0;
        for (i, d) in int.iter().enumerate() {
            out.push(digit(*d));
            let left = int.len() - 1 - i;
            if group && left > 0 && left.is_multiple_of(self.grouping_size as usize) {
                out.push(grouping);
            }
        }
        if !frac.is_empty() || self.decimal_separator_always_shown {
            out.push(decimal);
        }
        out.extend(frac.iter().map(|d| digit(*d)));
        if let Some(e) = exponent {
            out.push_str(&s.exponent_separator);
            if e < 0 {
                out.push_str(&s.minus_text());
            }
            let text = e.unsigned_abs().to_string();
            let min = self.exponent_digits.unwrap_or(1).max(0) as usize;
            for _ in text.len()..min {
                out.push(digit(b'0'));
            }
            out.extend(text.bytes().map(digit));
        }
        out
    }

    // ---------------------------------------------------------------------------------------------------------
    // Parsing

    /// `parse(text)` from the start; `None` when nothing could be parsed.
    pub fn parse(&self, text: &str) -> Option<JValue> {
        self.parse_at(text, &mut ParsePosition::new(0))
    }

    /// `NumberFormat.parse(String)`: `ParseException` when the text does not start with a number.
    pub fn parse_checked(&self, text: &str) -> Result<JValue, JError> {
        self.parse(text).ok_or_else(|| JError::new("java.text.ParseException", format!("Unparseable number: \"{text}\"")))
    }

    /// `parse(text, pos)`: a `Long` when the value is an integer that fits (and not -0), a `Double` otherwise, a
    /// `BigDecimal` with `setParseBigDecimal(true)`; `NaN` and the infinities are always `Double`s.
    pub fn parse_at(&self, text: &str, pos: &mut ParsePosition) -> Option<JValue> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let start = pos.index;
        let fail = |pos: &mut ParsePosition, at: usize| {
            pos.error_index = at as i64;
            None
        };
        if start > units.len() {
            return fail(pos, start);
        }
        let starts_with = |at: usize, s: &str| {
            let s: Vec<u16> = s.encode_utf16().collect();
            units.len() >= at + s.len() && units[at..at + s.len()] == s[..]
        };
        let utf16_len = |s: &str| s.encode_utf16().count();
        let nan = &self.symbols.nan;
        if !nan.is_empty() && starts_with(start, nan) {
            pos.index = start + utf16_len(nan);
            return Some(JValue::Double(f64::NAN));
        }
        // Prefixes: the longer match wins; equal matches leave the choice to the suffixes.
        let (pp, np) = (&self.positive_prefix.text, &self.negative_prefix.text);
        let mut maybe_positive = starts_with(start, pp);
        let mut maybe_negative = starts_with(start, np);
        if maybe_positive && maybe_negative {
            match utf16_len(pp).cmp(&utf16_len(np)) {
                std::cmp::Ordering::Greater => maybe_negative = false,
                std::cmp::Ordering::Less => maybe_positive = false,
                std::cmp::Ordering::Equal => {}
            }
        }
        if !maybe_positive && !maybe_negative {
            return fail(pos, start);
        }
        let at = start + if maybe_positive { utf16_len(pp) } else { utf16_len(np) };
        let infinity = &self.symbols.infinity;
        let (number, end, integer_end) = if !infinity.is_empty() && starts_with(at, infinity) {
            (None, at + utf16_len(infinity), None)
        } else {
            match self.scan(&units, at) {
                Some((n, end, integer_end)) => (Some(n), end, integer_end),
                None => return fail(pos, start),
            }
        };
        // Suffixes: of the candidates left, those whose suffix follows; the longer suffix wins.
        let (ps, ns) = (&self.positive_suffix.text, &self.negative_suffix.text);
        let positive_ok = maybe_positive && starts_with(end, ps);
        let negative_ok = maybe_negative && starts_with(end, ns);
        let negative = match (positive_ok, negative_ok) {
            (false, false) => return fail(pos, end),
            (true, false) => false,
            (false, true) => true,
            (true, true) => utf16_len(ns) > utf16_len(ps),
        };
        // With setParseIntegerOnly the position stops at the decimal separator, even though the suffix was
        // looked for after the whole number (recorded JDK behaviour).
        pos.index = integer_end.unwrap_or(end + utf16_len(if negative { ns } else { ps }));
        Some(match number {
            None => {
                let positive_result = !negative ^ (self.multiplier < 0);
                JValue::Double(if positive_result { f64::INFINITY } else { f64::NEG_INFINITY })
            }
            Some(n) => self.number_value(n, negative),
        })
    }

    /// Reads digits, separators and an exponent at `at`: the number read, where it ends and, when only the integer
    /// part counts (`setParseIntegerOnly`), where that part ends; `None` without a digit.
    fn scan(&self, units: &[u16], at: usize) -> Option<(Scanned, usize, Option<usize>)> {
        let s = &self.symbols;
        let (decimal, grouping) =
            if self.currency { (s.monetary_decimal_separator, s.monetary_grouping_separator) } else { (s.decimal_separator, s.grouping_separator) };
        let unit = |c: char| if (c as u32) < 0x10000 { Some(c as u32 as u16) } else { None };
        let (decimal, grouping) = (unit(decimal), unit(grouping));
        let exponent: Vec<u16> = s.exponent_separator.encode_utf16().collect();
        let minus = unit(s.minus_sign);
        let digit_of = |u: u16| -> Option<u8> {
            let zero = s.zero_digit as u32;
            let c = u as u32;
            if c >= zero && c < zero + 10 {
                return Some((c - zero) as u8);
            }
            char::from_u32(c).and_then(unicode_digit).map(|d| d as u8)
        };
        let mut n = Scanned::default();
        let mut i = at;
        let mut seen_digit = false;
        let mut seen_decimal = false;
        let mut before_grouping: Option<usize> = None;
        let mut frozen: Option<(Scanned, usize)> = None;
        while i < units.len() {
            let u = units[i];
            if let Some(d) = digit_of(u) {
                seen_digit = true;
                before_grouping = None;
                if d == 0 && n.digits.is_empty() {
                    if seen_decimal {
                        n.point -= 1;
                    }
                } else {
                    n.digits.push(b'0' + d);
                    if !seen_decimal {
                        n.point += 1;
                    }
                }
                i += 1;
            } else if Some(u) == decimal && !seen_decimal {
                if self.parse_integer_only {
                    if !seen_digit {
                        break;
                    }
                    // The value is the integer part; the rest is read only to find the suffix.
                    frozen = Some((std::mem::take(&mut n), i));
                }
                seen_decimal = true;
                before_grouping = None;
                i += 1;
            } else if Some(u) == grouping && self.grouping_used && !seen_decimal {
                before_grouping.get_or_insert(i);
                i += 1;
            } else if !exponent.is_empty() && units.len() >= i + exponent.len() && units[i..i + exponent.len()] == exponent[..] {
                // An exponent counts only when digits follow it; otherwise the number ends before it.
                let mut j = i + exponent.len();
                let negative = minus.is_some() && units.get(j).copied() == minus;
                if negative {
                    j += 1;
                }
                let digits_start = j;
                let mut e: i64 = 0;
                while j < units.len() {
                    match digit_of(units[j]) {
                        Some(d) => {
                            e = e.saturating_mul(10).saturating_add(d as i64).min(i64::MAX / 4);
                            j += 1;
                        }
                        None => break,
                    }
                }
                if j > digits_start && seen_digit {
                    n.point = n.point.saturating_add(if negative { -e } else { e });
                    before_grouping = None;
                    i = j;
                }
                break;
            } else {
                break;
            }
        }
        if let Some(b) = before_grouping {
            i = b;
        }
        if !seen_digit {
            return None;
        }
        n.point = n.point.clamp(i32::MIN as i64, i32::MAX as i64);
        match frozen {
            Some((mut int, stop)) => {
                int.point = int.point.clamp(i32::MIN as i64, i32::MAX as i64);
                Some((int, i, Some(stop)))
            }
            None => Some((n, i, None)),
        }
    }

    fn number_value(&self, n: Scanned, negative: bool) -> JValue {
        let m = self.multiplier;
        if self.parse_big_decimal {
            let unscaled: BigInt = if n.digits.is_empty() { BigInt::zero() } else { std::str::from_utf8(&n.digits).unwrap_or("0").parse().unwrap_or_default() };
            let scale = (n.digits.len() as i32).wrapping_sub(n.point as i32) as i64;
            let value = BigDec { unscaled, scale };
            let value = if negative { value.negate() } else { value };
            if m == 0 {
                return JValue::Double(n.to_f64(negative) / 0.0);
            }
            let value = if m == 1 { value } else { value.divide_int(m as i64, self.rounding) };
            return JValue::BigDecimal(value.to_java_string().into());
        }
        let zero = n.digits.is_empty();
        let mut result = if zero && negative && !self.parse_integer_only {
            Number::Double(-0.0)
        } else {
            match n.to_i64(negative) {
                Some(l) => Number::Long(l),
                None => Number::Double(n.to_f64(negative)),
            }
        };
        if m != 1 {
            result = match result {
                Number::Long(l) if m != 0 && (l % m as i64 == 0 || self.parse_integer_only) && !(l == i64::MIN && m == -1) => {
                    Number::Long(l / m as i64)
                }
                Number::Long(l) => integral(l as f64 / m as f64),
                Number::Double(d) => integral(d / m as f64),
            };
        }
        match result {
            Number::Long(l) => JValue::Long(l),
            Number::Double(d) => JValue::Double(d),
        }
    }
}

enum Number {
    Long(i64),
    Double(f64),
}

/// A quotient as the `Long` it equals when it is an integer within range (and not -0), else as a `Double`.
fn integral(d: f64) -> Number {
    if d.is_finite() && d.fract() == 0.0 && !(d == 0.0 && d.is_sign_negative()) && (-9.223_372_036_854_776e18..9.223_372_036_854_776e18).contains(&d) {
        Number::Long(d as i64)
    } else {
        Number::Double(d)
    }
}

/// The digits a parse read: `0.digits × 10^point`, without leading zeros (trailing ones kept for `BigDecimal`).
#[derive(Debug, Default)]
struct Scanned {
    digits: Vec<u8>,
    point: i64,
}

impl Scanned {
    fn to_f64(&self, negative: bool) -> f64 {
        let v = if self.digits.is_empty() {
            0.0
        } else {
            format!("0.{}e{}", std::str::from_utf8(&self.digits).unwrap_or("0"), self.point).parse::<f64>().unwrap_or(f64::NAN)
        };
        if negative { -v } else { v }
    }

    /// The value as a `long` when it is an integer in range.
    fn to_i64(&self, negative: bool) -> Option<i64> {
        let significant = self.digits.len() - self.digits.iter().rev().take_while(|d| **d == b'0').count();
        if self.digits.is_empty() {
            return Some(0);
        }
        if self.point < significant as i64 || self.point > 19 {
            return None;
        }
        let mut text = String::from_utf8(self.digits.clone()).ok()?;
        while (text.len() as i64) < self.point {
            text.push('0');
        }
        text.truncate(self.point as usize);
        let magnitude: i128 = text.parse().ok()?;
        let v = if negative { -magnitude } else { magnitude };
        i64::try_from(v).ok()
    }
}

fn not_a_number() -> JError {
    JError::new("java.lang.IllegalArgumentException", "Cannot format given Object as a Number")
}

/// The first code point of each run of ten Unicode decimal digits (`Character.digit(cp, 10) == 0`), as recorded
/// from JDK 21's `Character` data.
const DIGIT_ZEROS: [u32; 68] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090,
    0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950,
    0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16A60, 0x16AC0, 0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0,
    0x1E950, 0x1FBF0,
];

/// `Character.digit(ch, 10)` for a decimal digit of any script; `None` for anything else.
pub(crate) fn unicode_digit(ch: char) -> Option<u32> {
    let c = ch as u32;
    let i = DIGIT_ZEROS.partition_point(|z| *z <= c);
    let zero = DIGIT_ZEROS[i.checked_sub(1)?];
    (c - zero < 10).then_some(c - zero)
}

/// The `NumberFormat` factories.
pub struct NumberFormat;

impl NumberFormat {
    /// `NumberFormat.getNumberInstance(locale)`.
    pub fn number_instance(locale: &Locale) -> DecimalFormat {
        Self::instance(locale, 0)
    }

    /// `NumberFormat.getIntegerInstance(locale)`: no fraction digits (half-even rounding to an integer), parsing
    /// stops at the decimal separator.
    pub fn integer_instance(locale: &Locale) -> DecimalFormat {
        let mut f = Self::instance(locale, 1);
        f.set_maximum_fraction_digits(0);
        f.set_decimal_separator_always_shown(false);
        f.set_parse_integer_only(true);
        f
    }

    /// `NumberFormat.getCurrencyInstance(locale)`, with the fraction digits of the locale's currency.
    pub fn currency_instance(locale: &Locale) -> DecimalFormat {
        let mut f = Self::instance(locale, 2);
        let (min, max) = super::symbols::currency_fraction_digits(locale, (f.min_frac, f.max_frac));
        f.min_frac = min;
        f.max_frac = max;
        f
    }

    /// `NumberFormat.getPercentInstance(locale)`.
    pub fn percent_instance(locale: &Locale) -> DecimalFormat {
        Self::instance(locale, 3)
    }

    fn instance(locale: &Locale, kind: usize) -> DecimalFormat {
        let pattern = number_patterns(locale)[kind];
        DecimalFormat::with_locale(pattern, locale).unwrap_or_else(|_| DecimalFormat::with_locale("#,##0.###", locale).expect("valid pattern"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_of_other_scripts() {
        assert_eq!(unicode_digit('7'), Some(7));
        assert_eq!(unicode_digit('\u{663}'), Some(3));
        assert_eq!(unicode_digit('\u{FF19}'), Some(9));
        assert_eq!(unicode_digit('a'), None);
        assert_eq!(unicode_digit('\u{2F}'), None);
        assert_eq!(unicode_digit('\u{1D7CE}'), Some(0));
    }

    #[test]
    fn suffix_digits_continue_the_number() {
        let f = DecimalFormat::new("0x0", &DecimalFormatSymbols::root()).unwrap();
        assert_eq!((f.to_pattern(), f.format_i64(5).unwrap()), ("#00x".into(), "05x".into()));
        let f = DecimalFormat::new("0E0E", &DecimalFormatSymbols::root()).unwrap();
        assert_eq!((f.to_pattern(), f.format_i64(-12).unwrap()), ("0E0E;0E0".into(), "1E1".into()));
    }
}
