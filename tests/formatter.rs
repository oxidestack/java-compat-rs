//! java.util.Formatter (String.format) against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::lang::JValue;
use java_compat::text::{Locale, string_format};

#[test]
fn string_format_matches_the_jdk() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "formatter").as_array().unwrap() {
            let l = locale(c[0].as_str().unwrap(), data);
            let fmt = c[1].as_str().unwrap();
            let args: Vec<_> = c[2].as_array().unwrap().iter().map(value).collect();
            let got = match string_format(&l, fmt, &args) {
                Ok(s) => s,
                Err(e) => format!("ERROR {e}"),
            };
            let want = match &c[3] {
                serde_json::Value::String(s) => s.clone(),
                other => format!("ERROR {}", other["error"].as_str().unwrap()),
            };
            m.check(|| format!("{provider} [{l}] {fmt:?} {args:?}"), &got, &want);
        }
        m.finish(provider);
    }
}

/// `Byte` and `Short` arguments (JDK: `String.format("%x", (byte) -1)` is `ff`).
#[test]
fn byte_and_short_arguments() {
    let l = Locale::default_locale();
    let f = |fmt: &str, v: JValue| string_format(&l, fmt, &[v]);
    assert_eq!(f("%x", JValue::Byte(-1)).unwrap(), "ff");
    assert_eq!(f("%X", JValue::Short(-2)).unwrap(), "FFFE");
    assert_eq!(f("%o", JValue::Short(-1)).unwrap(), "177777");
    assert_eq!(f("%d", JValue::Byte(-5)).unwrap(), "-5");
    assert_eq!(f("%,d", JValue::Short(12345)).unwrap(), "12,345");
    assert_eq!(f("%c", JValue::Byte(65)).unwrap(), "A");
    let e = f("%c", JValue::Byte(-1)).unwrap_err();
    assert_eq!((e.class.as_str(), e.message.as_deref()), ("java.util.IllegalFormatCodePointException", Some("Code point = 0xffffffff")));
}
