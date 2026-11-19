//! Shared helpers for the fixture-driven tests: fixtures are recorded by `reference/run.sh` from a full JDK 26
//! (`tests/fixtures/full`) and from the MockServer image's trimmed JDK 26 (`tests/fixtures/javabase`).

#![allow(dead_code)]

use java_compat::lang::JValue;
use java_compat::text::{Locale, LocaleData};
use serde_json::Value;

pub const PROVIDERS: [(&str, LocaleData); 2] = [("full", LocaleData::Full), ("javabase", LocaleData::JavaBase)];

pub fn fixture(provider: &str, name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{provider}/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A locale from its `toString()` form.
pub fn locale(s: &str, data: LocaleData) -> Locale {
    let mut parts = s.splitn(3, '_');
    let l = parts.next().unwrap_or("");
    let c = parts.next().unwrap_or("");
    let v = parts.next().unwrap_or("");
    Locale::new(l, c, v).with_data(data)
}

/// A `{"t": type, "v": text}` fixture value.
pub fn value(v: &Value) -> JValue {
    let t = v["t"].as_str().unwrap();
    let text = v["v"].as_str().unwrap_or("");
    match t {
        "null" => JValue::Null,
        "int" => JValue::Int(text.parse().unwrap()),
        "long" => JValue::Long(text.parse().unwrap()),
        "double" => JValue::Double(parse_double(text)),
        "float" => JValue::Float(match text {
            "NaN" => f32::NAN,
            "Infinity" => f32::INFINITY,
            "-Infinity" => f32::NEG_INFINITY,
            t => t.parse().unwrap(),
        }),
        "bigint" => JValue::BigInteger(text.into()),
        "bigdec" => JValue::BigDecimal(text.into()),
        "string" => JValue::str(text),
        "bool" => JValue::Bool(text == "true"),
        "char" => JValue::Char(text.chars().next().unwrap()),
        "date" => date_value(text.parse().unwrap()),
        other => panic!("unknown value type {other}"),
    }
}

pub fn parse_double(text: &str) -> f64 {
    match text {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        t => t.parse().unwrap(),
    }
}

/// The fixture form of a result value.
pub fn describe(v: &JValue) -> Value {
    let (t, text) = match v {
        JValue::Null => return serde_json::json!({"t": "null"}),
        JValue::Int(i) => ("int", i.to_string()),
        JValue::Long(l) => ("long", l.to_string()),
        JValue::Double(d) => ("double", java_compat::double_to_string(*d)),
        JValue::BigDecimal(s) => ("bigdec", s.to_string()),
        JValue::BigInteger(s) => ("bigint", s.to_string()),
        JValue::Str(s) => ("string", s.to_string()),
        other => ("other", other.to_java_string()),
    };
    serde_json::json!({"t": t, "v": text})
}

/// `class: message` of an error, as the fixtures record exceptions.
pub fn error_text(e: &java_compat::lang::JError) -> String {
    e.to_string()
}

/// Collects mismatches and fails once with all of them (up to a limit).
#[derive(Default)]
pub struct Mismatches {
    pub checked: usize,
    pub failures: Vec<String>,
}

impl Mismatches {
    pub fn check(&mut self, what: impl FnOnce() -> String, got: &str, want: &str) {
        self.checked += 1;
        if got != want {
            self.failures.push(format!("{}\n     got: {got:?}\n    want: {want:?}", what()));
        }
    }

    pub fn finish(self, name: &str) {
        if !self.failures.is_empty() {
            let n = self.failures.len();
            let limit = std::env::var("MISMATCH_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
            let shown: Vec<String> = self.failures.into_iter().take(limit).collect();
            panic!("{name}: {n} of {} checks failed:\n{}", self.checked, shown.join("\n"));
        }
        assert!(self.checked > 0, "{name}: nothing checked");
        eprintln!("{name}: {} checks passed", self.checked);
    }
}

pub fn date_value(millis: i64) -> JValue {
    java_compat::util::Date::value(millis)
}
