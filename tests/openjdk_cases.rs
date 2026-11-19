//! Classic OpenJDK regression cases (DecimalFormat, SimpleDateFormat, MessageFormat, ChoiceFormat, Formatter),
//! recreated as input -> output checks and recorded from JDK 26 (`reference/src/OpenJdkCases.java`).

mod common;

use common::*;
use java_compat::lang::{JError, JValue};
use java_compat::text::{ChoiceFormat, DecimalFormat, MessageFormat, ParsePosition, RoundingMode, SimpleDateFormat, string_format};
use java_compat::util::TimeZone;
use serde_json::Value;

fn apply(f: &mut DecimalFormat, op: &str, arg: &JValue) {
    let int = || match arg {
        JValue::Int(i) => *i,
        _ => panic!("int"),
    };
    match op {
        "setGroupingSize" => f.set_grouping_size(int()).unwrap(),
        "setMinimumIntegerDigits" => f.set_minimum_integer_digits(int()),
        "setMinimumFractionDigits" => f.set_minimum_fraction_digits(int()),
        "setMaximumFractionDigits" => f.set_maximum_fraction_digits(int()),
        "setMultiplier" => f.set_multiplier(int()),
        "setRoundingMode" => f.set_rounding_mode(RoundingMode::from_name(arg.as_str().unwrap()).unwrap()),
        "setParseIntegerOnly" => f.set_parse_integer_only(matches!(arg, JValue::Bool(true))),
        other => panic!("{other}"),
    }
}

fn result(r: Result<String, JError>) -> String {
    match r {
        Ok(s) => s,
        Err(e) => format!("ERROR {e}"),
    }
}

fn expected(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => format!("ERROR {}", other["error"].as_str().unwrap_or("?")),
    }
}

#[test]
fn openjdk_regression_cases() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "openjdk_cases").as_array().unwrap() {
            let kind = c[0].as_str().unwrap();
            let id = c[1].as_str().unwrap();
            let what = || format!("{provider} {kind} {id}");
            match kind {
                "df" | "dfparse" => {
                    let l = locale(c[2].as_str().unwrap(), data);
                    let mut f = DecimalFormat::with_locale(c[3].as_str().unwrap(), &l).unwrap();
                    for s in c[4].as_array().unwrap() {
                        apply(&mut f, s[0].as_str().unwrap(), &value(&s[1]));
                    }
                    if kind == "df" {
                        m.check(what, &result(f.format(&value(&c[5]))), &expected(&c[6]));
                    } else {
                        let mut pos = ParsePosition::new(0);
                        let r = f.parse_at(c[5].as_str().unwrap(), &mut pos).map(|v| describe(&v)).unwrap_or(serde_json::json!({"t": "null"}));
                        m.check(what, &format!("{r} {} {}", pos.index, pos.error_index), &format!("{} {} {}", c[6], c[7], c[8]));
                    }
                }
                "sdf" => {
                    let l = locale(c[2].as_str().unwrap(), data);
                    let mut f = SimpleDateFormat::new(c[3].as_str().unwrap(), &l, &TimeZone::get(c[4].as_str().unwrap())).unwrap();
                    m.check(what, &f.format(c[5].as_str().unwrap().parse().unwrap()), &expected(&c[6]));
                }
                "sdfparse" => {
                    let l = locale(c[2].as_str().unwrap(), data);
                    let mut f = SimpleDateFormat::new(c[3].as_str().unwrap(), &l, &TimeZone::get(c[4].as_str().unwrap())).unwrap();
                    f.set_lenient(c[5].as_bool().unwrap());
                    f.set_2digit_year_start(c[6].as_str().unwrap().parse().unwrap());
                    let mut pos = ParsePosition::new(0);
                    let r = f.parse_at(c[7].as_str().unwrap(), &mut pos);
                    m.check(what, &format!("{:?} {} {}", r.map(|t| t.to_string()), pos.index, pos.error_index), &format!("{:?} {} {}", c[8].as_str(), c[9], c[10]));
                }
                "mf" => {
                    let l = locale(c[2].as_str().unwrap(), data);
                    let args: Vec<JValue> = c[4].as_array().unwrap().iter().map(value).collect();
                    let r = MessageFormat::new(c[3].as_str().unwrap(), &l).and_then(|f| f.format(&args));
                    m.check(what, &result(r), &expected(&c[5]));
                }
                "choice" => {
                    let f = ChoiceFormat::new(c[2].as_str().unwrap()).unwrap();
                    m.check(what, &result(f.format_f64(parse_double(c[3].as_str().unwrap()))), &expected(&c[4]));
                    m.check(what, &f.to_pattern(), &expected(&c[5]));
                }
                "fmt" => {
                    let l = locale(c[2].as_str().unwrap(), data);
                    let args: Vec<JValue> = c[4].as_array().unwrap().iter().map(value).collect();
                    m.check(what, &result(string_format(&l, c[3].as_str().unwrap(), &args)), &expected(&c[5]));
                }
                other => panic!("{other}"),
            }
        }
        m.finish(provider);
    }
}
