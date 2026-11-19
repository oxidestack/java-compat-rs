//! DateFormatSymbols, DateFormat, SimpleDateFormat and Date.toString against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::text::{DateFormat, DateFormatSymbols, Jdk, Locale, ParsePosition, SimpleDateFormat};
use java_compat::util::{Date, TimeZone};

#[test]
fn date_format_symbols() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "date_symbols").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let s = DateFormatSymbols::new(&l);
            let list = |k: &str| c[k].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect::<Vec<_>>().join("|");
            let own = |v: &[&str], pad_front: bool, pad_back: bool| {
                let mut out: Vec<String> = v.iter().map(|s| s.to_string()).collect();
                if pad_front {
                    out.insert(0, String::new());
                }
                if pad_back {
                    out.push(String::new());
                }
                out.join("|")
            };
            m.check(|| format!("{provider} [{l}] eras"), &own(&s.eras, false, false), &list("eras"));
            m.check(|| format!("{provider} [{l}] months"), &own(&s.months, false, true), &list("months"));
            m.check(|| format!("{provider} [{l}] shortMonths"), &own(&s.short_months, false, true), &list("shortMonths"));
            m.check(|| format!("{provider} [{l}] weekdays"), &own(&s.weekdays, true, false), &list("weekdays"));
            m.check(|| format!("{provider} [{l}] shortWeekdays"), &own(&s.short_weekdays, true, false), &list("shortWeekdays"));
            m.check(|| format!("{provider} [{l}] ampm"), &own(&s.am_pm, false, false), &list("ampm"));
        }
        m.finish(provider);
    }
}

#[test]
fn date_format_instances() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "date_instances").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let (ds, ts) = (c["dateStyle"].as_i64().unwrap() as i32, c["timeStyle"].as_i64().unwrap() as i32);
            let opt = |s: i32| if s < 0 { None } else { Some(s) };
            let mut f = DateFormat::instance(opt(ds), opt(ts), &l, &TimeZone::gmt()).unwrap();
            let what = format!("{provider} [{l}] date {ds} time {ts}");
            m.check(|| format!("{what} pattern"), &f.to_pattern(), c["pattern"].as_str().unwrap());
            for fm in c["formats"].as_array().unwrap() {
                f.set_time_zone(&TimeZone::get(fm[0].as_str().unwrap()));
                let t: i64 = fm[1].as_str().unwrap().parse().unwrap();
                m.check(|| format!("{what} {} {t}", fm[0]), &f.format(t), fm[2].as_str().unwrap());
            }
        }
        m.finish(provider);
    }
}

#[test]
fn simple_date_format_patterns() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "date_patterns").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let pattern = c["pattern"].as_str().unwrap();
            let created = SimpleDateFormat::new(pattern, &l, &TimeZone::gmt());
            if c.get("error").is_some() {
                let got = created.err().map(|e| e.to_string());
                m.check(|| format!("{provider} {pattern:?} error"), &format!("{got:?}"), &format!("{:?}", c["error"].as_str()));
                continue;
            }
            let mut f = created.unwrap();
            let what = format!("{provider} [{l}] {pattern:?}");
            m.check(|| format!("{what} toPattern"), &f.to_pattern(), c["toPattern"].as_str().unwrap());
            let localized = match f.to_localized_pattern() {
                Ok(p) => p,
                Err(e) => format!("ERROR {e}"),
            };
            m.check(|| format!("{what} toLocalizedPattern"), &localized, c["localized"].as_str().unwrap());
            for fm in c["formats"].as_array().unwrap() {
                f.set_time_zone(&TimeZone::get(fm[0].as_str().unwrap()));
                let t: i64 = fm[1].as_str().unwrap().parse().unwrap();
                m.check(|| format!("{what} {} {t}", fm[0]), &f.format(t), fm[2].as_str().unwrap());
            }
        }
        m.finish(provider);
    }
}

#[test]
fn date_to_string() {
    let mut m = Mismatches::default();
    for c in fixture("full", "date_tostring").as_array().unwrap() {
        let zone = TimeZone::get(c["zone"].as_str().unwrap());
        let t: i64 = c["time"].as_str().unwrap().parse().unwrap();
        let d = Date(t);
        m.check(|| format!("{} {t} toString", zone.id()), &d.to_string_in(&zone), c["toString"].as_str().unwrap());
        m.check(|| format!("{} {t} toGMTString", zone.id()), &d.to_gmt_string_in(&zone), c["toGMTString"].as_str().unwrap());
        m.check(|| format!("{t} hashCode"), &d.java_hash_code().to_string(), &c["hashCode"].to_string());
    }
    m.finish("Date");
}

#[test]
fn simple_date_format_parse() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "date_parse").as_array().unwrap() {
            let l = locale(c["locale"].as_str().unwrap(), data);
            let pattern = c["pattern"].as_str().unwrap();
            let zone = TimeZone::get(c["zone"].as_str().unwrap());
            let lenient = c["lenient"].as_bool().unwrap();
            let default_century = c.get("defaultCentury").is_some();
            let mut f = SimpleDateFormat::new(pattern, &l, &zone).unwrap();
            if !default_century {
                f.set_2digit_year_start(-1577923200000);
            }
            f.set_lenient(lenient);
            for case in c["cases"].as_array().unwrap() {
                f.set_time_zone(&zone);
                let text = case["text"].as_str().unwrap();
                let index = case["index"].as_u64().unwrap() as usize;
                let mut pos = ParsePosition::new(index);
                let r = f.parse_at(text, &mut pos);
                let got = format!("{:?} pos={} err={} zone={}", r.map(|t| t.to_string()), pos.index, pos.error_index, f.time_zone().id());
                let want = format!("{:?} pos={} err={} zone={}", case["result"].as_str(), case["pos"], case["err"], case["zoneAfter"].as_str().unwrap());
                m.check(|| format!("{provider} [{l}] {} lenient={lenient} {pattern:?} parse {text:?}", zone.id()), &got, &want);
            }
        }
        m.finish(provider);
    }
}

/// Lenient parsing of a space separator literal, per JDK release. JDK 26 (JDK-8324665, JDK 23+) lets any
/// SPACE_SEPARATOR match another; JDK 21 needs the exact character. Expected values: JDK 25.0.2 and Keycloak
/// 26.5.3's JDK 21.0.10, lenient `DateFormat.getTimeInstance(MEDIUM, Locale.US)` (`h:mm:ss a`) and
/// `new SimpleDateFormat("yyyy MM", Locale.US)`, both in UTC.
#[test]
fn lenient_space_separators_per_jdk() {
    let utc = TimeZone::get("UTC");
    let run = |jdk: Option<Jdk>, pattern: Option<&str>, text: &str| {
        let mut f = match pattern {
            Some(p) => SimpleDateFormat::new(p, &Locale::us(), &utc).unwrap(),
            None => DateFormat::time_instance(2, &Locale::us(), &utc).unwrap(),
        };
        if let Some(jdk) = jdk {
            f.set_jdk(jdk);
        }
        let mut pos = ParsePosition::new(0);
        let r = f.parse_at(text, &mut pos);
        format!("{r:?} pos={} err={}", pos.index, pos.error_index)
    };
    let time = [("2:54:13 PM", "None pos=0 err=7"), ("2:54:13\u{202f}PM", "Some(53653000) pos=10 err=-1"), ("2:54:13\u{a0}PM", "None pos=0 err=7")];
    let year = [("2020 05", "Some(1588291200000) pos=7 err=-1"), ("2020\u{a0}05", "None pos=0 err=4"), ("2020\u{202f}05", "None pos=0 err=4")];
    for (text, jdk21) in time {
        assert_eq!(run(None, None, text), "Some(53653000) pos=10 err=-1", "default (JDK 26) {text:?}");
        assert_eq!(run(Some(Jdk::V26), None, text), "Some(53653000) pos=10 err=-1", "JDK 26 {text:?}");
        assert_eq!(run(Some(Jdk::V21), None, text), jdk21, "JDK 21 {text:?}");
    }
    for (text, jdk21) in year {
        assert_eq!(run(Some(Jdk::V26), Some("yyyy MM"), text), "Some(1588291200000) pos=7 err=-1", "JDK 26 {text:?}");
        assert_eq!(run(Some(Jdk::V21), Some("yyyy MM"), text), jdk21, "JDK 21 {text:?}");
    }
}
