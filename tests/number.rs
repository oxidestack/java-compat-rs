//! DecimalFormatSymbols, DecimalFormat and NumberFormat against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::lang::JValue;
use java_compat::text::{DecimalFormat, DecimalFormatSymbols, NumberFormat, ParsePosition, RoundingMode};
use serde_json::Value;

fn expected_format(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => format!("ERROR {}", other["error"].as_str().unwrap()),
    }
}

fn actual_format(f: &DecimalFormat, v: &JValue) -> String {
    match f.format(v) {
        Ok(s) => s,
        Err(e) => format!("ERROR {e}"),
    }
}

fn check_formats(m: &mut Mismatches, what: &str, f: &DecimalFormat, formats: &Value) {
    for pair in formats.as_array().unwrap() {
        let v = value(&pair[0]);
        m.check(|| format!("{what} format {v:?}"), &actual_format(f, &v), &expected_format(&pair[1]));
    }
}

fn check_parse(m: &mut Mismatches, what: &str, f: &DecimalFormat, case: &Value) {
    let text = case["text"].as_str().unwrap();
    let index = case["index"].as_u64().unwrap() as usize;
    let mut pos = ParsePosition::new(index);
    let got = f.parse_at(text, &mut pos).map(|v| describe(&v)).unwrap_or(serde_json::json!({"t": "null"}));
    let got = format!("{got} pos={} err={}", pos.index, pos.error_index);
    let want = format!("{} pos={} err={}", case["result"], case["pos"], case["err"]);
    m.check(|| format!("{what} parse {text:?} at {index}"), &got, &want);
}

#[test]
fn decimal_format_symbols() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for s in fixture(provider, "number_symbols").as_array().unwrap() {
            let l = locale(s["locale"].as_str().unwrap(), data);
            let d = DecimalFormatSymbols::new(&l);
            let got = format!(
                "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                d.zero_digit, d.grouping_separator, d.decimal_separator, d.per_mill, d.percent, d.digit, d.pattern_separator, d.infinity, d.nan,
                d.minus_sign, d.currency_symbol, d.international_currency_symbol, d.monetary_decimal_separator, d.monetary_grouping_separator,
                d.exponent_separator
            );
            let f = |k: &str| s[k].as_str().unwrap().to_string();
            let want = [
                "zero", "grouping", "decimal", "perMill", "percent", "digit", "patternSeparator", "infinity", "nan", "minus", "currencySymbol",
                "intlCurrencySymbol", "monetaryDecimal", "monetaryGrouping", "exponent",
            ]
            .map(f)
            .join(" ");
            m.check(|| format!("{provider} symbols {l}"), &got, &want);
        }
        m.finish(provider);
    }
}

#[test]
fn number_format_instances() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "number_instances").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let kind = c["kind"].as_str().unwrap();
            let f = match kind {
                "number" => NumberFormat::number_instance(&l),
                "integer" => NumberFormat::integer_instance(&l),
                "currency" => NumberFormat::currency_instance(&l),
                _ => NumberFormat::percent_instance(&l),
            };
            let what = format!("{provider} {kind} [{l}]");
            m.check(|| format!("{what} toPattern"), &f.to_pattern(), c["pattern"].as_str().unwrap());
            m.check(|| format!("{what} toLocalizedPattern"), &f.to_localized_pattern(), c["localizedPattern"].as_str().unwrap());
            let digits = format!("{} {} {} {} {}", f.maximum_fraction_digits(), f.minimum_fraction_digits(), f.maximum_integer_digits(), f.minimum_integer_digits(), f.is_parse_integer_only());
            let want = format!("{} {} {} {} {}", c["maxFrac"], c["minFrac"], c["maxInt"], c["minInt"], c["integerOnly"]);
            m.check(|| format!("{what} digits"), &digits, &want);
            check_formats(&mut m, &what, &f, &c["formats"]);
            for p in c["parses"].as_array().unwrap() {
                check_parse(&mut m, &what, &f, p);
            }
        }
        m.finish(provider);
    }
}

#[test]
fn decimal_format_patterns() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "decimal_patterns").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let pattern = c["pattern"].as_str().unwrap();
            let f = DecimalFormat::with_locale(pattern, &l).unwrap();
            let what = format!("{provider} [{l}] {pattern:?}");
            m.check(|| format!("{what} toPattern"), &f.to_pattern(), c["toPattern"].as_str().unwrap());
            m.check(|| format!("{what} toLocalizedPattern"), &f.to_localized_pattern(), c["toLocalizedPattern"].as_str().unwrap());
            let got = format!(
                "{:?} {:?} {:?} {:?} {} {} {} {} {} {} {} {}",
                f.positive_prefix(), f.positive_suffix(), f.negative_prefix(), f.negative_suffix(), f.multiplier(), f.grouping_size(),
                f.is_grouping_used(), f.is_decimal_separator_always_shown(), f.maximum_integer_digits(), f.minimum_integer_digits(),
                f.maximum_fraction_digits(), f.minimum_fraction_digits()
            );
            let s = |k: &str| format!("{:?}", c[k].as_str().unwrap());
            let want = format!(
                "{} {} {} {} {} {} {} {} {} {} {} {}",
                s("posPrefix"), s("posSuffix"), s("negPrefix"), s("negSuffix"), c["multiplier"], c["groupingSize"], c["groupingUsed"],
                c["decimalAlways"], c["maxInt"], c["minInt"], c["maxFrac"], c["minFrac"]
            );
            m.check(|| format!("{what} properties"), &got, &want);
            check_formats(&mut m, &what, &f, &c["formats"]);
        }
        m.finish(provider);
    }
}

#[test]
fn decimal_format_pattern_errors() {
    let mut m = Mismatches::default();
    for c in fixture("full", "decimal_pattern_errors").as_array().unwrap() {
        if let Some(p) = c["pattern"].as_str() {
            let got = match DecimalFormat::with_locale(p, &locale("en_US", java_compat::text::LocaleData::Full)) {
                Ok(f) => f.to_pattern(),
                Err(e) => format!("ERROR {e}"),
            };
            let want = match c["error"].as_str() {
                Some(e) => format!("ERROR {e}"),
                None => c["toPattern"].as_str().unwrap().to_string(),
            };
            m.check(|| format!("pattern {p:?}"), &got, &want);
        } else {
            let p = c["localizedPattern"].as_str().unwrap();
            let mut f = DecimalFormat::with_locale("0", &locale("fr_FR", java_compat::text::LocaleData::Full)).unwrap();
            let got = match f.apply_localized_pattern(p) {
                Ok(()) => format!("{} {}", f.to_pattern(), f.format_f64(-1234.5678).unwrap()),
                Err(e) => format!("ERROR {e}"),
            };
            let want = match c["error"].as_str() {
                Some(e) => format!("ERROR {e}"),
                None => format!("{} {}", c["toPattern"].as_str().unwrap(), c["format"].as_str().unwrap()),
            };
            m.check(|| format!("localized pattern {p:?}"), &got, &want);
        }
    }
    m.finish("pattern errors");
}

fn apply(f: &mut DecimalFormat, op: &str, arg: &JValue) -> Result<(), java_compat::lang::JError> {
    let int = || match arg {
        JValue::Int(i) => *i,
        _ => panic!("int arg"),
    };
    let text = || arg.as_str().unwrap().to_string();
    let boolean = || matches!(arg, JValue::Bool(true));
    match op {
        "setMaximumFractionDigits" => f.set_maximum_fraction_digits(int()),
        "setMinimumFractionDigits" => f.set_minimum_fraction_digits(int()),
        "setMaximumIntegerDigits" => f.set_maximum_integer_digits(int()),
        "setMinimumIntegerDigits" => f.set_minimum_integer_digits(int()),
        "setGroupingSize" => f.set_grouping_size(int())?,
        "setGroupingUsed" => f.set_grouping_used(boolean()),
        "setMultiplier" => f.set_multiplier(int()),
        "setDecimalSeparatorAlwaysShown" => f.set_decimal_separator_always_shown(boolean()),
        "setPositivePrefix" => f.set_positive_prefix(&text()),
        "setNegativePrefix" => f.set_negative_prefix(&text()),
        "setPositiveSuffix" => f.set_positive_suffix(&text()),
        "setNegativeSuffix" => f.set_negative_suffix(&text()),
        "setRoundingMode" => f.set_rounding_mode(RoundingMode::from_name(&text()).unwrap()),
        other => panic!("{other}"),
    }
    Ok(())
}

#[test]
fn decimal_format_setters() {
    let mut m = Mismatches::default();
    for c in fixture("full", "decimal_setters").as_array().unwrap() {
        let pattern = c["pattern"].as_str().unwrap();
        let op = c["op"].as_str().unwrap();
        let arg = value(&c["arg"]);
        let mut f = DecimalFormat::with_locale(pattern, &locale("en_US", java_compat::text::LocaleData::Full)).unwrap();
        let err = apply(&mut f, op, &arg).err().map(|e| e.to_string());
        let what = format!("{pattern:?} {op}({arg:?})");
        m.check(|| format!("{what} error"), &format!("{err:?}"), &format!("{:?}", c["error"].as_str().map(String::from)));
        m.check(|| format!("{what} toPattern"), &f.to_pattern(), c["toPattern"].as_str().unwrap());
        let got = format!("{} {} {} {}", f.maximum_integer_digits(), f.minimum_integer_digits(), f.maximum_fraction_digits(), f.minimum_fraction_digits());
        let want = format!("{} {} {} {}", c["maxInt"], c["minInt"], c["maxFrac"], c["minFrac"]);
        m.check(|| format!("{what} digits"), &got, &want);
        check_formats(&mut m, &what, &f, &c["formats"]);
    }
    m.finish("setters");
}

#[test]
fn decimal_format_rounding_modes() {
    let mut m = Mismatches::default();
    for c in fixture("full", "decimal_rounding").as_array().unwrap() {
        let pattern = c["pattern"].as_str().unwrap();
        let mode = c["mode"].as_str().unwrap();
        let mut f = DecimalFormat::with_locale(pattern, &locale("en_US", java_compat::text::LocaleData::Full)).unwrap();
        f.set_rounding_mode(RoundingMode::from_name(mode).unwrap());
        check_formats(&mut m, &format!("{mode} {pattern:?}"), &f, &c["formats"]);
    }
    m.finish("rounding");
}

#[test]
fn decimal_format_parse() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "decimal_parse").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let pattern = c["pattern"].as_str().unwrap();
            let mut f = DecimalFormat::with_locale(pattern, &l).unwrap();
            let mut what = format!("{provider} [{l}] {pattern:?}");
            match c["variant"].as_i64() {
                Some(1) => f.set_parse_integer_only(true),
                Some(2) => f.set_parse_big_decimal(true),
                Some(3) => f.set_grouping_used(false),
                _ => {}
            }
            if let Some(v) = c["variant"].as_i64() {
                what.push_str(&format!(" variant {v}"));
            }
            if let Some(mult) = c["multiplier"].as_i64() {
                f.set_multiplier(mult as i32);
                f.set_parse_big_decimal(c["bigDecimal"].as_bool().unwrap());
                what.push_str(&format!(" x{mult} big={}", c["bigDecimal"]));
            }
            for case in c["cases"].as_array().unwrap() {
                check_parse(&mut m, &what, &f, case);
            }
        }
        m.finish(provider);
    }
}
