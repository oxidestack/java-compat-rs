//! The decimal arithmetic behind [`super::DecimalFormat`]: `java.math.RoundingMode`, a `BigDecimal` value
//! ([`BigDec`]) and an unsigned decimal digit string ([`Decimal`]) that is rounded to a number of digits.
//!
//! Written from the Java SE API specification of `RoundingMode`, `BigDecimal` and `DecimalFormat` and from
//! behaviour recorded in real JDKs (`tests/fixtures/**`, jshell on JDK 21); see `docs/provenance.md`.
//!
//! A `double` is formatted from its shortest decimal representation (`Double.toString`'s digits, [`split`]).
//! Where that representation differs from the binary value, the difference only matters when the digits cut
//! off by rounding are exactly one half; [`Decimal::round`] then decides by which side of the half the binary
//! value lies (JDK 8+ behaviour, `new DecimalFormat("0.000")` with `HALF_UP` formats `0.8055` as `0.805`).

use num_bigint::{BigInt, Sign};
use num_traits::{Signed, Zero};

use crate::lang::JError;

pub(crate) use crate::number::split;

/// `java.math.RoundingMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoundingMode {
    /// `UP`.
    Up,
    /// `DOWN`.
    Down,
    /// `CEILING`.
    Ceiling,
    /// `FLOOR`.
    Floor,
    /// `HALF_UP`.
    HalfUp,
    /// `HALF_DOWN`.
    HalfDown,
    /// `HALF_EVEN`, `DecimalFormat`'s default.
    HalfEven,
    /// `UNNECESSARY`.
    Unnecessary,
}

const MODES: [(RoundingMode, &str); 8] = [
    (RoundingMode::Up, "UP"),
    (RoundingMode::Down, "DOWN"),
    (RoundingMode::Ceiling, "CEILING"),
    (RoundingMode::Floor, "FLOOR"),
    (RoundingMode::HalfUp, "HALF_UP"),
    (RoundingMode::HalfDown, "HALF_DOWN"),
    (RoundingMode::HalfEven, "HALF_EVEN"),
    (RoundingMode::Unnecessary, "UNNECESSARY"),
];

impl RoundingMode {
    /// The constant's name (`Enum.name()`).
    pub fn name(self) -> &'static str {
        MODES.iter().find(|(m, _)| *m == self).map(|(_, n)| *n).unwrap_or("HALF_EVEN")
    }

    /// `RoundingMode.valueOf(name)`; `None` for an unknown name.
    pub fn from_name(name: &str) -> Option<RoundingMode> {
        MODES.iter().find(|(_, n)| *n == name).map(|(m, _)| *m)
    }

    /// Whether a magnitude is increased (rounded away from zero) when the discarded part compares to one half of
    /// the last kept unit as `half` (`Less`, `Equal`, `Greater`) and is non-zero; `last_odd` is the parity of the
    /// last kept digit. `None` for `UNNECESSARY`.
    fn rounds_away(self, negative: bool, half: std::cmp::Ordering, last_odd: bool) -> Option<bool> {
        use std::cmp::Ordering::*;
        Some(match self {
            RoundingMode::Up => true,
            RoundingMode::Down => false,
            RoundingMode::Ceiling => !negative,
            RoundingMode::Floor => negative,
            RoundingMode::HalfUp => half != Less,
            RoundingMode::HalfDown => half == Greater,
            RoundingMode::HalfEven => half == Greater || half == Equal && last_odd,
            RoundingMode::Unnecessary => return None,
        })
    }
}

/// The `ArithmeticException` `UNNECESSARY` raises when digits would be dropped.
pub(crate) fn rounding_needed() -> JError {
    JError::new("java.lang.ArithmeticException", "Rounding needed with the rounding mode being set to RoundingMode.UNNECESSARY")
}

/// Where a `double`'s binary value lies relative to the decimal digits that stand for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exactness {
    /// The digits are the value (integers, `BigDecimal`s, exactly representable doubles).
    Exact,
    /// The digits are slightly above the binary value.
    Above,
    /// The digits are slightly below the binary value.
    Below,
}

/// An unsigned decimal `0.d₁d₂…dₙ × 10^point`: `digits` are ASCII without leading or trailing zeros, so zero is
/// the empty list. `point` is the number of integer digits (negative for values below 0.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Decimal {
    pub digits: Vec<u8>,
    pub point: i64,
    exactness: Exactness,
}

impl Decimal {
    /// A decimal from ASCII digits standing for `0.digits × 10^point`, normalised.
    pub fn new(digits: &[u8], point: i64) -> Decimal {
        let lead = digits.iter().take_while(|d| **d == b'0').count();
        let rest = &digits[lead..];
        let len = rest.len() - rest.iter().rev().take_while(|d| **d == b'0').count();
        let digits = rest[..len].to_vec();
        let point = if digits.is_empty() { 0 } else { point - lead as i64 };
        Decimal { digits, point, exactness: Exactness::Exact }
    }

    /// The magnitude of an integer given as decimal text (no sign).
    pub fn from_integer_text(text: &str) -> Decimal {
        Decimal::new(text.as_bytes(), text.len() as i64)
    }

    /// The magnitude of a `BigDecimal`.
    pub fn from_big_dec(value: &BigDec) -> Decimal {
        let text = value.unscaled.magnitude().to_string();
        Decimal::new(text.as_bytes(), text.len() as i64 - value.scale)
    }

    /// The magnitude of a finite `double`, as its shortest decimal representation.
    pub fn from_f64(value: f64) -> Decimal {
        if value == 0.0 {
            return Decimal::new(&[], 0);
        }
        let s = split(value.abs());
        let mut d = Decimal::new(&s.digits, s.exp as i64 + s.digits.len() as i64);
        d.exactness = match (s.exact, s.away) {
            (true, _) => Exactness::Exact,
            (false, true) => Exactness::Above,
            (false, false) => Exactness::Below,
        };
        d
    }

    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// Keeps the first `keep` significant digits (none when `keep <= 0`, everything when `keep >= len`), rounding
    /// the magnitude of a number of sign `negative` by `mode`.
    pub fn round(&mut self, keep: i64, mode: RoundingMode, negative: bool) -> Result<(), JError> {
        use std::cmp::Ordering::*;
        let len = self.digits.len() as i64;
        if keep >= len {
            return Ok(());
        }
        if keep < 0 {
            // Recorded JDK behaviour: a value below a tenth of the last kept unit becomes zero in every mode,
            // UP, CEILING and UNNECESSARY included.
            *self = Decimal::new(&[], 0);
            return Ok(());
        }
        // How the dropped digits compare with half a unit of the last kept digit.
        let half = {
            let first = self.digits[keep as usize];
            match first.cmp(&b'5') {
                Equal if keep + 1 < len => Greater,
                Equal => match self.exactness {
                    Exactness::Exact => Equal,
                    Exactness::Above => Less,
                    Exactness::Below => Greater,
                },
                other => other,
            }
        };
        let last_odd = keep > 0 && (self.digits[keep as usize - 1] - b'0') % 2 == 1;
        let away = mode.rounds_away(negative, half, last_odd).ok_or_else(rounding_needed)?;
        let unit_point = self.point - keep; // one unit of the last kept digit is 10^unit_point
        let mut kept: Vec<u8> = self.digits[..keep as usize].to_vec();
        if away {
            // Add one unit of the last kept position.
            let mut i = kept.len();
            loop {
                if i == 0 {
                    kept.insert(0, b'1');
                    self.point = unit_point + kept.len() as i64;
                    break;
                }
                i -= 1;
                if kept[i] == b'9' {
                    kept[i] = b'0';
                } else {
                    kept[i] += 1;
                    self.point = unit_point + kept.len() as i64;
                    break;
                }
            }
        }
        let point = self.point;
        *self = Decimal::new(&kept, point);
        Ok(())
    }

    /// The digits before the decimal point, zero-filled (empty below 1).
    pub fn integer_digits(&self) -> Vec<u8> {
        if self.point <= 0 {
            return Vec::new();
        }
        let p = self.point as usize;
        let mut out: Vec<u8> = self.digits.iter().take(p).copied().collect();
        out.resize(p, b'0');
        out
    }

    /// The significant digits after the decimal point, with the zeros between the point and them.
    pub fn fraction_digits(&self) -> Vec<u8> {
        if self.point >= 0 {
            return self.digits.iter().skip(self.point as usize).copied().collect();
        }
        let mut out = vec![b'0'; (-self.point) as usize];
        out.extend_from_slice(&self.digits);
        out
    }
}

/// A `java.math.BigDecimal`: `unscaled × 10^-scale`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BigDec {
    pub unscaled: BigInt,
    pub scale: i64,
}

impl BigDec {
    /// `new BigDecimal(text)`: an optional sign, digits with an optional decimal point, an optional exponent
    /// (`e`/`E`, optional sign, digits). `None` when `text` is not such a number.
    pub fn parse(s: &str) -> Option<BigDec> {
        let (negative, rest) = match s.as_bytes().first()? {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        let (mantissa, exponent) = match rest.find(['e', 'E']) {
            Some(i) => {
                let e = &rest[i + 1..];
                let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                (&rest[..i], e.parse::<i64>().ok()?)
            }
            None => (rest, 0),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if int.is_empty() && frac.is_empty() || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
            return None;
        }
        let magnitude: BigInt = format!("{int}{frac}").parse().ok()?;
        let unscaled = if negative { -magnitude } else { magnitude };
        Some(BigDec { unscaled, scale: frac.len() as i64 - exponent })
    }

    pub fn is_negative(&self) -> bool {
        self.unscaled.is_negative()
    }

    pub fn negate(&self) -> BigDec {
        BigDec { unscaled: -&self.unscaled, scale: self.scale }
    }

    pub fn abs(&self) -> BigDec {
        BigDec { unscaled: self.unscaled.abs(), scale: self.scale }
    }

    /// `this.multiply(BigDecimal.valueOf(m))`: the scale is kept.
    pub fn multiply_int(&self, m: i64) -> BigDec {
        BigDec { unscaled: &self.unscaled * m, scale: self.scale }
    }

    /// `this.divide(BigDecimal.valueOf(m))` when the quotient has a finite decimal expansion (at the smallest scale
    /// not below this one that holds it exactly), otherwise `this.divide(BigDecimal.valueOf(m), scale(), rounding)`.
    /// `m` must not be zero.
    pub fn divide_int(&self, m: i64, rounding: RoundingMode) -> BigDec {
        assert!(m != 0, "division by zero");
        let divisor = BigInt::from(m);
        let mut numerator = self.unscaled.clone();
        // A quotient by an i64 terminates within 64 more decimal places, or never.
        for extra in 0..=64 {
            if (&numerator % &divisor).is_zero() {
                return BigDec { unscaled: numerator / &divisor, scale: self.scale + extra };
            }
            numerator *= 10;
        }
        BigDec { unscaled: round_quotient(&self.unscaled, &divisor, rounding), scale: self.scale }
    }

    /// `BigDecimal.toString()`.
    pub fn to_java_string(&self) -> String {
        crate::number::big_decimal_to_string(&format!("{}E{}", self.unscaled, -self.scale)).unwrap_or_else(|| self.unscaled.to_string())
    }
}

/// `n / d` rounded to an integer by `mode` (`UNNECESSARY` truncates: callers only reach it for exact quotients).
fn round_quotient(n: &BigInt, d: &BigInt, mode: RoundingMode) -> BigInt {
    let q = n / d; // truncates toward zero
    let r = n - &q * d;
    if r.is_zero() {
        return q;
    }
    let negative = (n.sign() == Sign::Minus) != (d.sign() == Sign::Minus);
    let half = (r.abs() * 2u32).cmp(&d.abs());
    let last_odd = (&q % 2u32) != BigInt::zero();
    let away = mode.rounds_away(negative, half, last_odd).unwrap_or(false);
    if !away {
        q
    } else if negative {
        q - 1
    } else {
        q + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortest_decimal_matches_double_to_string() {
        let d = split(2e23);
        assert_eq!((String::from_utf8(d.digits).unwrap(), d.exp, d.exact, d.away), ("20".into(), 22, false, true));
        let d = split(5e-324);
        assert_eq!((String::from_utf8(d.digits).unwrap(), d.exp), ("49".into(), -325));
        let d = split(100.0);
        assert_eq!((String::from_utf8(d.digits).unwrap(), d.exp, d.exact), ("100".into(), 0, true));
    }

    #[test]
    fn shortest_decimal_fixture() {
        // FormattedFPDecimal.split in JDK 26 for random doubles (reference/src/ShortestDecimalFixtures.java).
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/full/shortest_decimal.json");
        let cases: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut bad = Vec::new();
        for c in cases.as_array().unwrap() {
            let d = f64::from_bits(u64::from_str_radix(c[0].as_str().unwrap(), 16).unwrap());
            let r = split(d);
            // Normalize away trailing zeros: only the value reaches DigitList.
            let norm = |digits: &str, exp: i64| {
                let t = digits.trim_end_matches('0');
                (t.to_string(), exp + (digits.len() - t.len()) as i64)
            };
            let (gd, ge) = norm(&String::from_utf8(r.digits.clone()).unwrap(), r.exp as i64);
            let (wd, we) = norm(c[1].as_str().unwrap(), c[3].as_i64().unwrap());
            let got = format!("{gd} {ge} {} {}", r.exact, r.away);
            let want = format!("{wd} {we} {} {}", c[4], c[5]);
            if got != want {
                bad.push(format!("{d:e}: got {got}, want {want}"));
            }
        }
        assert!(bad.is_empty(), "{} mismatches: {:?}", bad.len(), &bad[..bad.len().min(10)]);
    }

    #[test]
    fn rounding_follows_the_binary_value_at_a_half() {
        let fmt = |d: Decimal| (String::from_utf8(d.digits.clone()).unwrap(), d.point);
        // 0.8055 is 0.80549999…: HALF_UP keeps 805.
        let mut d = Decimal::from_f64(0.8055);
        d.round(3, RoundingMode::HalfUp, false).unwrap();
        assert_eq!(fmt(d), ("805".into(), 0));
        // 2.5 is exact: HALF_EVEN goes to 2, HALF_UP to 3.
        let mut d = Decimal::from_f64(2.5);
        d.round(1, RoundingMode::HalfEven, false).unwrap();
        assert_eq!(fmt(d), ("2".into(), 1));
        let mut d = Decimal::from_f64(9.96);
        d.round(2, RoundingMode::HalfUp, false).unwrap();
        assert_eq!(fmt(d), ("1".into(), 2));
        let mut d = Decimal::from_f64(0.0004);
        d.round(-2, RoundingMode::Up, false).unwrap();
        assert!(d.is_zero());
        let mut d = Decimal::from_f64(0.004);
        d.round(0, RoundingMode::Up, false).unwrap();
        assert_eq!(fmt(d), ("1".into(), -1));
        assert!(Decimal::from_f64(0.5).round(0, RoundingMode::Unnecessary, false).is_err());
    }

    #[test]
    fn big_decimal_division() {
        let b = |s: &str| BigDec::parse(s).unwrap();
        assert_eq!(b("14").divide_int(100, RoundingMode::HalfEven).to_java_string(), "0.14");
        assert_eq!(b("3.5E2").divide_int(7, RoundingMode::HalfEven).to_java_string(), "5E+1");
        assert_eq!(b("10").divide_int(7, RoundingMode::HalfEven).to_java_string(), "1");
        assert_eq!(b("1.5").divide_int(7, RoundingMode::HalfEven).to_java_string(), "0.2");
        assert_eq!(b("-0.000123456").to_java_string(), "-0.000123456");
        assert_eq!(b("1E+5").to_java_string(), "1E+5");
    }
}
