//! `java.util.regex` against vectors recorded in Keycloak 26.5.3's OpenJDK 21 (`reference/run.sh regex`):
//! `Pattern.compile` errors, `matches`/`lookingAt`/`find` with groups, `replaceAll`/`replaceFirst`, `split` with
//! limits, and the `String` methods that delegate to them.

mod common;

use common::Mismatches;
use java_compat::lang::{JError, JValue};
use serde_json::{Value, json};

fn cases() -> Vec<Value> {
    let path = format!("{}/tests/fixtures/jdk21/regex.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str::<Value>(&text).unwrap().as_array().unwrap().clone()
}

fn outcome(r: Result<Value, JError>) -> Value {
    r.unwrap_or_else(|e| json!({"error": e.to_string()}))
}

fn string_call(input: &str, name: &str, args: &[JValue]) -> Result<JValue, JError> {
    java_compat::methods::invoke(&JValue::str(input), name, args).expect("String method").map(|v| v.expect("a value"))
}

fn strings(v: JValue) -> Value {
    match v {
        JValue::Array(a) => Value::Array(a.items().iter().map(|s| Value::String(s.to_java_string())).collect()),
        other => panic!("not an array: {other:?}"),
    }
}

/// `String.replaceAll`/`replaceFirst`/`split`/`matches` (the cases recorded with flags 0 call them).
#[test]
fn string_methods_match_jdk21() {
    let mut m = Mismatches::default();
    for c in cases() {
        if c["flags"] != 0 {
            continue;
        }
        let pattern = c["pattern"].as_str().unwrap();
        let p = JValue::str(pattern);
        let Some(inputs) = c["inputs"].as_array() else {
            // A syntax error: every String method throws it.
            let want = c["error"].as_str().unwrap();
            let got = outcome(string_call("a", "matches", std::slice::from_ref(&p)).map(|v| json!(v.to_java_string())));
            m.check(|| format!("{pattern:?} compile"), got["error"].as_str().unwrap_or("no error"), want);
            continue;
        };
        for i in inputs {
            let input = i["input"].as_str().unwrap();
            let got = outcome(string_call(input, "matches", std::slice::from_ref(&p)).map(|v| json!(matches!(v, JValue::Bool(true)))));
            m.check(|| format!("{pattern:?}.matches({input:?})"), &got.to_string(), &i["stringMatches"].to_string());
            for (limit, want) in i["split"].as_object().unwrap() {
                let l: i32 = limit.parse().unwrap();
                let got = outcome(string_call(input, "split", &[p.clone(), JValue::Int(l)]).map(strings));
                m.check(|| format!("{input:?}.split({pattern:?}, {l})"), &got.to_string(), &want.to_string());
            }
            for r in i["replace"].as_array().unwrap() {
                let rep = r[0].as_str().unwrap();
                for (k, name) in [(1, "replaceAll"), (2, "replaceFirst")] {
                    let got = outcome(string_call(input, name, &[p.clone(), JValue::str(rep)]).map(|v| json!(v.to_java_string())));
                    m.check(|| format!("{input:?}.{name}({pattern:?}, {rep:?})"), &got.to_string(), &r[k].to_string());
                }
            }
        }
    }
    m.finish("String regex methods (JDK 21)");
}

fn utf16_index(s: &str, byte: usize) -> usize {
    s[..byte].encode_utf16().count()
}

fn groups(s: &str, m: &java_compat::regex::Match) -> Value {
    let mut v = vec![json!([utf16_index(s, m.start()), utf16_index(s, m.end())])];
    v.extend(m.groups.iter().map(|g| g.map_or(Value::Null, |(a, b)| json!(&s[a..b]))));
    Value::Array(v)
}

/// `Pattern.compile(pattern, flags)` and its `Matcher`: every recorded case, under every flag combination.
#[test]
fn pattern_and_matcher_match_jdk21() {
    use java_compat::regex::Pattern;
    let mut m = Mismatches::default();
    for c in cases() {
        let pattern = c["pattern"].as_str().unwrap();
        let flags = c["flags"].as_u64().unwrap() as u32;
        let p = match Pattern::compile(pattern, flags) {
            Ok(p) => p,
            Err(e) => {
                let got = JError::from(e).to_string();
                m.check(|| format!("{pattern:?} /{flags:#x} compile"), &got, c["error"].as_str().unwrap_or("compiles"));
                continue;
            }
        };
        let Some(inputs) = c["inputs"].as_array() else {
            m.check(|| format!("{pattern:?} /{flags:#x} compile"), "compiles", c["error"].as_str().unwrap());
            continue;
        };
        m.check(|| format!("{pattern:?} /{flags:#x} groupCount"), &p.group_count().to_string(), &c["groupCount"].to_string());
        for i in inputs {
            let s = i["input"].as_str().unwrap();
            let what = |op: &str| format!("{pattern:?} /{flags:#x} {op} on {s:?}");
            let got = outcome(p.matches(s).map(|r| r.map_or(Value::Null, |x| groups(s, &x))));
            m.check(|| what("matches"), &got.to_string(), &i["matches"].to_string());
            let got = outcome(p.looking_at(s).map(|r| json!(r.is_some())));
            m.check(|| what("lookingAt"), &got.to_string(), &i["lookingAt"].to_string());
            let got = outcome(p.find_all(s).map(|all| Value::Array(all.iter().map(|x| groups(s, x)).collect())));
            m.check(|| what("find"), &got.to_string(), &i["find"].to_string());
            if flags != 0 {
                // Flags 0 cases are String-method ones, checked by string_methods_match_jdk21.
                for (limit, want) in i["split"].as_object().unwrap() {
                    let l: i32 = limit.parse().unwrap();
                    let got = outcome(p.split(s, l).map(|v| json!(v)));
                    m.check(|| what(&format!("split {l}")), &got.to_string(), &want.to_string());
                }
                for r in i["replace"].as_array().unwrap() {
                    let rep = r[0].as_str().unwrap();
                    for (k, all) in [(1, true), (2, false)] {
                        let got = outcome(p.replace(s, rep, all).map(|v| json!(v)));
                        m.check(|| what(&format!("replace({rep:?}, all={all})")), &got.to_string(), &r[k].to_string());
                    }
                }
            }
        }
    }
    m.finish("Pattern/Matcher (JDK 21)");
}

/// `methods::java_regex` and `expand_replacement`, the fancy-regex-level API: Java's syntax and replacement errors.
#[test]
fn fancy_regex_api_follows_jdk21() {
    use java_compat::methods::{expand_replacement, java_regex};
    let re = java_regex(r"(?<year>\d{4})-(\d\d)(x)?").unwrap();
    let caps = re.captures("on 2026-10-15").unwrap().unwrap();
    let expand = |r: &str| expand_replacement(&caps, r).map_err(|e| e.to_string());
    assert_eq!(expand("$2/${year}[$3]$0"), Ok("10/2026[]2026-10".to_string()));
    assert_eq!(expand("$23"), Ok("103".to_string()));
    assert_eq!(expand("a$").unwrap_err(), "java.lang.IllegalArgumentException: Illegal group reference: group index is missing");
    assert_eq!(expand("${}").unwrap_err(), "java.lang.IllegalArgumentException: named capturing group has 0 length name");
    assert_eq!(expand("${year").unwrap_err(), "java.lang.IllegalArgumentException: named capturing group is missing trailing '}'");
    assert_eq!(expand("${1a}").unwrap_err(), "java.lang.IllegalArgumentException: capturing group name {1a} starts with digit character");
    assert_eq!(expand("$4").unwrap_err(), "java.lang.IndexOutOfBoundsException: No group 4");
    assert_eq!(expand("\\").unwrap_err(), "java.lang.IllegalArgumentException: character to be escaped is missing");
    // `$` in Java allows a final line terminator; `.` excludes \u0085 and the other Java line terminators.
    assert!(java_regex("a$").unwrap().is_match("a\r\n").unwrap());
    assert!(!java_regex("a.b").unwrap().is_match("a\u{85}b").unwrap());
    assert_eq!(
        java_regex("a{,3}").unwrap_err().to_string(),
        "java.util.regex.PatternSyntaxException: Illegal repetition near index 2\na{,3}\n  ^"
    );
}
