//! Date symbols, `DateFormat` style patterns, zone names and week data of every locale of Keycloak's theme
//! languages (plus English and French) against the JDKs they were recorded from: JDK 26 (`full`), the MockServer
//! image's trimmed JDK (`javabase`, root data for every language but English) and Keycloak 26.5.3's JDK 21 (`jdk21`,
//! `LocaleData::FullJdk21`). The data is what `locale_dates.rs` is generated from; the formatted and parsed values
//! check that `SimpleDateFormat` and the `DateFormat` instances work with it as the JDK does (standalone month names,
//! locale digits, region week data, zone names).

mod common;

use common::*;
use java_compat::text::{DateFormat, DateFormatSymbols, LocaleData, ParsePosition, SimpleDateFormat, has_date_data};
use java_compat::util::{GregorianCalendar, TimeZone};
use serde_json::Value;

const LOCALE_PROVIDERS: [(&str, LocaleData); 3] = [
    ("full", LocaleData::Full),
    ("javabase", LocaleData::JavaBase),
    ("jdk21", LocaleData::FullJdk21),
];

fn strings(v: &Value) -> String {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect::<Vec<_>>().join("|")
}

fn join(v: &[&str]) -> String {
    v.join("|")
}

#[test]
fn locale_date_data() {
    let mut total = 0;
    for (provider, data) in LOCALE_PROVIDERS {
        let mut m = Mismatches::default();
        for r in fixture(provider, "locale_dates").as_array().unwrap() {
            let tag = r["locale"].as_str().unwrap();
            let l = locale(tag, data);
            if r["calendar"] != "GregorianCalendar" {
                // th_TH: the JDK gives it a Buddhist calendar, which this crate does not have.
                assert!(!has_date_data(&l), "{tag}");
                continue;
            }
            assert!(has_date_data(&l), "{provider} {tag}");
            let s = DateFormatSymbols::new(&l);
            for (key, got) in [
                ("eras", join(&s.eras)),
                ("months", join(&s.months)),
                ("shortMonths", join(&s.short_months)),
                ("standaloneMonths", join(&s.standalone_months)),
                ("standaloneShortMonths", join(&s.standalone_short_months)),
                ("weekdays", join(&s.weekdays)),
                ("shortWeekdays", join(&s.short_weekdays)),
                ("ampm", join(&s.am_pm)),
            ] {
                m.check(|| format!("{tag} {key}"), &got, &strings(&r[key]));
            }
            let gmt = TimeZone::gmt();
            for style in 0..4 {
                let date = DateFormat::date_instance(style, &l, &gmt).unwrap().to_pattern();
                m.check(|| format!("{tag} date {style}"), &date, r["date"][style as usize].as_str().unwrap());
                let time = DateFormat::time_instance(style, &l, &gmt).unwrap().to_pattern();
                m.check(|| format!("{tag} time {style}"), &time, r["time"][style as usize].as_str().unwrap());
                for t in 0..4 {
                    let dt = DateFormat::date_time_instance(style, t, &l, &gmt).unwrap().to_pattern();
                    let want = r["dateTime"][(style * 4 + t) as usize].as_str().unwrap();
                    m.check(|| format!("{tag} dateTime {style} {t}"), &dt, want);
                }
            }
            for z in r["zones"].as_array().unwrap() {
                let id = z[0].as_str().unwrap();
                let tz = TimeZone::get(id);
                for (i, (daylight, style)) in [(false, 1), (false, 0), (true, 1), (true, 0)].into_iter().enumerate() {
                    m.check(
                        || format!("{tag} zone {id} {daylight} {style}"),
                        &tz.display_name(daylight, style, &l),
                        z[i + 1].as_str().unwrap(),
                    );
                }
            }
            let fields = r["fields"].as_str().unwrap();
            for f in r["formats"].as_array().unwrap() {
                let (zone, millis) = (f[0].as_str().unwrap(), f[1].as_str().unwrap().parse::<i64>().unwrap());
                let mut sdf = SimpleDateFormat::new(fields, &l, &TimeZone::get(zone)).unwrap();
                m.check(|| format!("{tag} format {zone} {millis}"), &sdf.format(millis), f[2].as_str().unwrap());
            }
            // The style instances at 2026-07-15 06:05:09.123 GMT in Paris and 2026-02-15 06:05:09.123 in GMT.
            for f in r["styles"].as_array().unwrap() {
                let (a, b) = (f[0].as_i64().unwrap() as i32, f[1].as_i64().unwrap() as i32);
                let (a, b) = ((a >= 0).then_some(a), (b >= 0).then_some(b));
                for (zone, millis, want) in [("Europe/Paris", 1_784_095_509_123, &f[2]), ("GMT", 1_771_135_509_123, &f[3])] {
                    let mut sdf = DateFormat::instance(a, b, &l, &TimeZone::get(zone)).unwrap();
                    m.check(|| format!("{tag} style {a:?} {b:?} {zone}"), &sdf.format(millis), want.as_str().unwrap());
                }
            }
            for p in r["parses"].as_array().unwrap() {
                let (pattern, text) = (p[0].as_str().unwrap(), p[1].as_str().unwrap());
                let mut sdf = SimpleDateFormat::new(pattern, &l, &TimeZone::gmt()).unwrap();
                sdf.set_2digit_year_start(-1_577_923_200_000);
                let mut pos = ParsePosition::new(0);
                let got = sdf.parse_at(text, &mut pos);
                let got = format!("{} {} {}", got.map(|t| t.to_string()).unwrap_or("null".into()), pos.index, pos.error_index);
                let want = format!("{} {} {}", p[2].as_str().unwrap_or("null"), p[3], p[4]);
                m.check(|| format!("{tag} parse {pattern} {text:?}"), &got, &want);
            }
            total += 1;
        }
        m.finish(&format!("locale date data ({provider})"));
    }
    assert!(total > 600, "{total} locales");
}

#[test]
fn region_week_data() {
    for (provider, data) in LOCALE_PROVIDERS {
        let mut m = Mismatches::default();
        for w in fixture(provider, "week_data").as_array().unwrap() {
            let region = w[0].as_str().unwrap();
            let l = java_compat::text::Locale::new("", region, "").with_data(data);
            let c = GregorianCalendar::new(0, &TimeZone::gmt(), &l);
            m.check(
                || format!("{region} week data"),
                &format!("{} {}", c.first_day_of_week(), c.minimal_days_in_first_week()),
                &format!("{} {}", w[1], w[2]),
            );
        }
        m.finish(&format!("region week data ({provider})"));
    }
}
