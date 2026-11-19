//! MessageFormat and ChoiceFormat against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::text::{ChoiceFormat, MessageFormat};

#[test]
fn message_format_matches_the_jdk() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "message_format").as_array().unwrap() {
            let l = locale(c[0].as_str().unwrap(), data);
            let pattern = c[1].as_str().unwrap();
            // Not ported: ListFormat, java.time formatters and compact number formats.
            if ["{0,list}", "{0,dtf_date}", "{0,number,compact_short}"].contains(&pattern) {
                continue;
            }
            let args: Vec<_> = c[2].as_array().unwrap().iter().map(value).collect();
            let (got, got_pattern) = match MessageFormat::new(pattern, &l) {
                Ok(f) => (
                    match f.format(&args) {
                        Ok(s) => s,
                        Err(e) => format!("ERROR {e}"),
                    },
                    Some(f.to_pattern()),
                ),
                Err(e) => (format!("ERROR {e}"), None),
            };
            let want = match &c[3] {
                serde_json::Value::String(s) => s.clone(),
                other => format!("ERROR {}", other["error"].as_str().unwrap()),
            };
            m.check(|| format!("{provider} [{l}] {pattern:?} {args:?}"), &got, &want);
            m.check(|| format!("{provider} [{l}] {pattern:?} toPattern"), &format!("{got_pattern:?}"), &format!("{:?}", c[4].as_str()));
        }
        m.finish(provider);
    }
}

#[test]
fn choice_format_matches_the_jdk() {
    let mut m = Mismatches::default();
    for c in fixture("full", "choice_format").as_array().unwrap() {
        let pattern = c["pattern"].as_str().unwrap();
        match ChoiceFormat::new(pattern) {
            Err(e) => m.check(|| format!("{pattern:?}"), &format!("ERROR {e}"), &format!("ERROR {}", c["error"].as_str().unwrap_or("none"))),
            Ok(f) => {
                m.check(|| format!("{pattern:?} toPattern"), &f.to_pattern(), c["toPattern"].as_str().unwrap_or("ERROR"));
                for fm in c["formats"].as_array().unwrap() {
                    let d = parse_double(fm[0].as_str().unwrap());
                    let got = match f.format_f64(d) {
                        Ok(s) => s,
                        Err(e) => format!("ERROR {e}"),
                    };
                    let want = match &fm[1] {
                        serde_json::Value::String(s) => s.clone(),
                        other => format!("ERROR {}", other["error"].as_str().unwrap()),
                    };
                    m.check(|| format!("{pattern:?} format {d}"), &got, &want);
                }
            }
        }
    }
    m.finish("choice");
}
