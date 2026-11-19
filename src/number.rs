//! `java.lang.Double.toString` (JDK 19+, shortest-decimal algorithm), which Jackson uses for every `double`.

use num_bigint::BigUint;

/// `jdk.internal.math.FormattedFPDecimal.split(v)` for a finite, non-zero `|v|`: `Double.toString`'s shortest
/// decimal `f × 10^e` (JDK 19+ semantics: at least two digits when one would do, closest to `v`), whether it
/// equals `v` exactly, and whether it lies above `v` ("rounded away").
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shortest {
    /// The significand's decimal digits (ASCII), trailing zeros possible.
    pub digits: Vec<u8>,
    /// The exponent of the last digit.
    pub exp: i32,
    pub exact: bool,
    pub away: bool,
}

pub(crate) fn split(v: f64) -> Shortest {
    let v = v.abs();
    let bits = v.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (m, q) = if biased == 0 { (fraction, -1074) } else { (fraction | (1u64 << 52), biased - 1075) };
    // Integers below 2^53 are their own shortest decimal (DoubleToDecimal's fast path).
    if (-52..0).contains(&q) && (m >> -q) << -q == m {
        let digits = (m >> -q).to_string().into_bytes();
        return Shortest { digits, exp: 0, exact: true, away: false };
    }
    let mut sci = format!("{v:e}");
    if !sci.contains('.') {
        // One digit would do: Java picks the closest two-digit decimal instead.
        sci = format!("{v:.1e}");
    }
    let (mantissa, exponent) = sci.split_once('e').expect("exponent");
    let point_exp: i32 = exponent.parse().expect("integer exponent");
    let digits: Vec<u8> = mantissa.bytes().filter(|c| *c != b'.').collect();
    let exp = point_exp - (digits.len() as i32 - 1);
    let f: BigUint = String::from_utf8_lossy(&digits).parse().expect("digits");
    // Scale f × 10^exp and m × 2^q to integers: d = f·a, x = m·b.
    let ten = |n: i32| BigUint::from(10u32).pow(n as u32);
    let mut a = BigUint::from(1u32);
    let mut b = BigUint::from(1u32);
    if exp >= 0 {
        a *= ten(exp);
    } else {
        b *= ten(-exp);
    }
    if q >= 0 {
        b <<= q as usize;
    } else {
        a <<= (-q) as usize;
    }
    let x = BigUint::from(m) * &b;
    let d = &f * &a;
    let ord = d.cmp(&x);
    // Rust breaks a tie between two equally close shortest candidates upwards; Java picks the even one.
    if ord == std::cmp::Ordering::Greater && digits.last().is_some_and(|l| (l - b'0') % 2 == 1) {
        let below = (&f - 1u32) * &a;
        if &d - &x == &x - &below {
            let mut digits = (&f - 1u32).to_string().into_bytes();
            let mut exp = exp;
            while digits.len() > 1 && digits.last() == Some(&b'0') {
                digits.pop();
                exp += 1;
            }
            return Shortest { digits, exp, exact: false, away: false };
        }
    }
    if ord == std::cmp::Ordering::Less && digits.last().is_some_and(|l| (l - b'0') % 2 == 1) {
        let above = (&f + 1u32) * &a;
        if &above - &x == &x - &d {
            let mut digits = (&f + 1u32).to_string().into_bytes();
            let mut exp = exp;
            while digits.len() > 1 && digits.last() == Some(&b'0') {
                digits.pop();
                exp += 1;
            }
            return Shortest { digits, exp, exact: false, away: true };
        }
    }
    Shortest { digits, exp, exact: ord == std::cmp::Ordering::Equal, away: ord == std::cmp::Ordering::Greater }
}

/// `Double.toString(d)`: the shortest decimal that rounds to `d`, written plainly with at least one fractional
/// digit when `10^-3 <= |d| < 10^7`, otherwise as `d.dddE[-]n`.
pub fn double_to_string(d: f64) -> String {
    if d.is_nan() {
        return "NaN".into();
    }
    if d.is_infinite() {
        return if d > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if d == 0.0 {
        return if d.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let shortest = split(d);
    let all = String::from_utf8(shortest.digits).expect("ascii digits");
    let exponent = shortest.exp + all.len() as i32 - 1;
    let digits = match all.trim_end_matches('0') {
        "" => "0".to_string(),
        trimmed => trimmed.to_string(),
    };
    let sign = if d < 0.0 { "-" } else { "" };
    let abs = d.abs();
    if (1e-3..1e7).contains(&abs) {
        let point = exponent + 1; // digits before the decimal point
        let (int, frac) = if point <= 0 {
            ("0".to_string(), format!("{}{digits}", "0".repeat((-point) as usize)))
        } else if point as usize >= digits.len() {
            (format!("{digits}{}", "0".repeat(point as usize - digits.len())), "0".to_string())
        } else {
            (digits[..point as usize].to_string(), digits[point as usize..].to_string())
        };
        format!("{sign}{int}.{frac}")
    } else {
        let frac = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{sign}{}.{frac}E{exponent}", &digits[..1])
    }
}

/// `java.lang.Float.toString`, same rules at `float` precision.
pub fn float_to_string(f: f32) -> String {
    if f.is_nan() || f.is_infinite() || f == 0.0 {
        return double_to_string(f as f64);
    }
    let mut sci = format!("{:e}", f.abs());
    if !sci.contains('.') {
        sci = format!("{:.1e}", f.abs());
    }
    let (mantissa, exponent) = sci.split_once('e').expect("{:e} always has an exponent");
    let exponent: i32 = exponent.parse().expect("integer exponent");
    // Re-use the double formatter on the float's shortest digits.
    let shortest: f64 = format!("{mantissa}e{exponent}").parse().expect("valid float text");
    double_to_string(if f < 0.0 { -shortest } else { shortest })
}

#[cfg(test)]
mod tests {
    use super::{double_to_string, float_to_string};

    #[test]
    fn matches_java() {
        for (d, s) in [
            (1.5, "1.5"),
            (1.0, "1.0"),
            (100.0, "100.0"),
            (0.001, "0.001"),
            (0.0001, "1.0E-4"),
            (1234567.0, "1234567.0"),
            (1e7, "1.0E7"),
            (12345678.9, "1.23456789E7"),
            (1e21, "1.0E21"),
            (-2.5e-5, "-2.5E-5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (f64::MAX, "1.7976931348623157E308"),
            (f64::MIN_POSITIVE, "2.2250738585072014E-308"),
            (4.9e-324, "4.9E-324"),
            (-0.0, "-0.0"),
            (0.75, "0.75"),
            (0.8, "0.8"),
            // Two equally close shortest decimals: Java takes the even one.
            (688_661_853_876_637.2, "6.886618538766372E14"),
            (2e23, "2.0E23"),
        ] {
            assert_eq!(double_to_string(d), s, "{d}");
        }
        assert_eq!(float_to_string(0.1), "0.1");
        assert_eq!(float_to_string(1e10), "1.0E10");
    }
}

/// `new java.math.BigDecimal(text).toString()` for a decimal literal (`-12.5`, `1.0E-7`, `3e+2`): plain notation
/// when the scale is non-negative and the adjusted exponent is at least -6, otherwise `d.dddE±n`. `None` when
/// `text` is not a decimal number.
pub fn big_decimal_to_string(text: &str) -> Option<String> {
    let (negative, rest) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let (mantissa, exponent) = match rest.find(['e', 'E']) {
        Some(i) => (&rest[..i], rest[i + 1..].parse::<i64>().ok()?),
        None => (rest, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int.is_empty() && frac.is_empty() || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let scale = frac.len() as i64 - exponent;
    let digits = format!("{int}{frac}");
    let unscaled = match digits.trim_start_matches('0') {
        "" => "0",
        d => d,
    };
    let sign = if negative && unscaled != "0" { "-" } else { "" };
    let adjusted = -scale + (unscaled.len() as i64 - 1);
    let body = if scale == 0 {
        unscaled.to_string()
    } else if scale > 0 && adjusted >= -6 {
        let scale = scale as usize;
        if unscaled.len() > scale {
            format!("{}.{}", &unscaled[..unscaled.len() - scale], &unscaled[unscaled.len() - scale..])
        } else {
            format!("0.{}{unscaled}", "0".repeat(scale - unscaled.len()))
        }
    } else {
        let exp = if adjusted >= 0 { format!("+{adjusted}") } else { adjusted.to_string() };
        if unscaled.len() > 1 { format!("{}.{}E{exp}", &unscaled[..1], &unscaled[1..]) } else { format!("{unscaled}E{exp}") }
    };
    Some(format!("{sign}{body}"))
}

#[cfg(test)]
mod big_decimal_tests {
    use super::big_decimal_to_string as bd;

    #[test]
    fn matches_java() {
        for (input, expected) in [
            ("1.0E-5", "0.000010"),
            ("1.23456789E7", "12345678.9"),
            ("1.0E-7", "1.0E-7"),
            ("1.0E21", "1.0E+21"),
            ("0.1", "0.1"),
            ("123.45", "123.45"),
            ("100.0", "100.0"),
            ("-0.0", "0.0"),
            ("0.003", "0.003"),
            ("1.7976931348623157E308", "1.7976931348623157E+308"),
            ("1.2345678901234567E19", "1.2345678901234567E+19"),
            ("10", "10"),
            ("-12.5", "-12.5"),
            ("1E2", "1E+2"),
            ("5E-324", "5E-324"),
        ] {
            assert_eq!(bd(input).as_deref(), Some(expected), "{input}");
        }
        assert_eq!(bd("abc"), None);
    }
}
