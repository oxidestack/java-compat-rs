//! `java.text` and the locale-sensitive parts of `java.util`, reproducing JDK 26 byte for byte:
//!
//! - [`Locale`] (with commons-lang3 `LocaleUtils.toLocale` as [`Locale::parse`]) and the locale data provider it
//!   resolves from, [`LocaleData`]: a full JDK ([`LocaleData::Full`], CLDR incl. French) or only what
//!   `java.base` carries ([`LocaleData::JavaBase`], root and English, as in trimmed runtimes), or a full JDK 21
//!   ([`LocaleData::FullJdk21`], whose number data differs from JDK 26's in a few locales);
//! - numbers: [`DecimalFormatSymbols`], [`DecimalFormat`] (pattern grammar, formatting of every numeric
//!   [`JValue`](crate::lang::JValue), parsing), the [`NumberFormat`] factories;
//! - [`message_format`] / [`MessageFormat`] with [`ChoiceFormat`];
//! - [`string_format`], `java.util.Formatter`;
//! - dates: [`SimpleDateFormat`] (format and lenient parse; [`Jdk`] selects JDK 21's exact space matching) and
//!   the [`DateFormat`] factories, over
//!   [`crate::util`]'s `Date`, `TimeZone` and `GregorianCalendar`;
//! - [`objects`]: `JObject` wrappers with registered class definitions for the values templates see.
//!
//! The default locale is `en` without country ([`Locale::default_locale`]) and the default time zone GMT
//! ([`crate::util::TimeZone::default_zone`]); nothing global is mutable: callers pass locales and zones.

pub mod date_format;
pub mod decimal;
mod digits;
pub mod formatter;
pub mod locale;
pub(crate) mod locale_dates;
mod locale_numbers;
pub mod message;
pub mod objects;
pub mod symbols;

pub use date_format::{DateFormat, Jdk, SimpleDateFormat};
pub use decimal::{DecimalFormat, NumberFormat, ParsePosition};
pub use digits::RoundingMode;
pub use formatter::string_format;
pub use locale::{Locale, LocaleData};
pub use message::{ChoiceFormat, MessageFormat, message_format};
pub use symbols::{DateFormatSymbols, DecimalFormatSymbols, has_date_data};

/// `(unscaledValue, scale)` of a `BigDecimal` literal.
pub(crate) fn big_decimal_parts(s: &str) -> Option<(num_bigint::BigInt, i64)> {
    digits::BigDec::parse(s).map(|d| (d.unscaled, d.scale))
}

/// `Number.longValue()` of a boxed number (`BigInteger`/`BigDecimal` truncate, then keep the low 64 bits).
pub(crate) fn number_long_value(v: &crate::lang::JValue) -> Option<i64> {
    use crate::lang::JValue;
    Some(match v {
        JValue::Int(i) => *i as i64,
        JValue::Long(l) => *l,
        JValue::Double(d) => *d as i64,
        JValue::Float(f) => *f as i64,
        JValue::BigInteger(s) => low_64_bits(&s.parse::<num_bigint::BigInt>().ok()?),
        JValue::BigDecimal(s) => {
            let d = digits::BigDec::parse(s)?;
            let int = if d.scale <= 0 {
                d.unscaled * num_bigint::BigInt::from(10u32).pow((-d.scale) as u32)
            } else {
                d.unscaled / num_bigint::BigInt::from(10u32).pow(d.scale as u32)
            };
            low_64_bits(&int)
        }
        _ => return None,
    })
}

fn low_64_bits(b: &num_bigint::BigInt) -> i64 {
    let (sign, digits) = b.to_u64_digits();
    let low = digits.first().copied().unwrap_or(0);
    let v = low as i64;
    if sign == num_bigint::Sign::Minus { v.wrapping_neg() } else { v }
}
