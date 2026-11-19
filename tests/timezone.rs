//! TimeZone lookup, offsets and display names against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::util::TimeZone;
use java_compat::util::timezone::{LONG, SHORT};

#[test]
fn time_zones() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "timezone").as_array().unwrap() {
            let input = c["input"].as_str().unwrap();
            let z = TimeZone::get(input);
            let what = format!("{provider} getTimeZone({input:?})");
            m.check(|| format!("{what} id"), z.id(), c["id"].as_str().unwrap());
            let got = format!("{} {} {}", z.raw_offset(), z.dst_savings(), z.use_daylight_time());
            let want = format!("{} {} {}", c["raw"], c["dstSavings"], c["usesDaylight"]);
            m.check(|| format!("{what} rules"), &got, &want);
            for o in c["offsets"].as_array().unwrap() {
                let t: i64 = o[0].as_str().unwrap().parse().unwrap();
                let got = format!("{} {}", z.offset(t), z.in_daylight_time(t));
                let want = format!("{} {}", o[1], o[2]);
                m.check(|| format!("{what} offset at {t}"), &got, &want);
            }
            for n in c["names"].as_array().unwrap() {
                let l = locale(n[0].as_str().unwrap(), data);
                let got = [(false, LONG), (false, SHORT), (true, LONG), (true, SHORT)].map(|(d, s)| z.display_name(d, s, &l)).join("|");
                let want = (1..5).map(|i| n[i].as_str().unwrap()).collect::<Vec<_>>().join("|");
                m.check(|| format!("{what} names [{l}]"), &got, &want);
            }
        }
        m.finish(provider);
    }
}

/// Zones the crate's name tables lack, against probes of JDK 26.0.2.1 (`LocaleData::Full`) and JDK 21.0.12.1
/// (`LocaleData::FullJdk21`) (full per-zone, per-locale probe tables): `Etc/GMT±N`
/// in the locale's GMT format, `Etc/GMT0` with the locale's zero GMT format as its short name, the UTC links, and the
/// zones at GMT all year (`None`: the JDK's daylight long name there is per city, which the tables do not carry).
#[test]
fn zones_without_recorded_names() {
    use java_compat::text::LocaleData::{Full, FullJdk21};
    let cases = [
        (Full, "en", "Etc/GMT-14", [Some("GMT+14:00"), Some("GMT+14:00"), Some("GMT+14:00"), Some("GMT+14:00")]),
        (Full, "fr", "Etc/GMT-14", [Some("UTC+14:00"), Some("GMT+14:00"), Some("UTC+14:00"), Some("GMT+14:00")]),
        (FullJdk21, "fr", "Etc/GMT-14", [Some("UTC+14:00"), Some("UTC+14:00"), Some("UTC+14:00"), Some("UTC+14:00")]),
        (Full, "ar_EG", "Etc/GMT+12", [Some("غرينتش-12:00"), Some("GMT-12:00"), Some("غرينتش-12:00"), Some("GMT-12:00")]),
        (FullJdk21, "ar_EG", "Etc/GMT+12", [Some("غرينتش-١٢:٠٠"); 4]),
        (Full, "fr", "Etc/GMT0", [Some("heure moyenne de Greenwich"), Some("GMT"), Some("heure moyenne de Greenwich"), Some("GMT")]),
        (FullJdk21, "fr", "Etc/GMT0", [Some("heure moyenne de Greenwich"), Some("UTC"), Some("heure moyenne de Greenwich"), Some("UTC")]),
        (FullJdk21, "ar", "Etc/GMT0", [Some("توقيت غرينتش"), Some("غرينتش"), Some("توقيت غرينتش"), Some("غرينتش")]),
        (Full, "en", "Etc/UCT", [Some("Coordinated Universal Time"), Some("UTC"), Some("Coordinated Universal Time"), Some("UTC")]),
        (Full, "en", "Africa/Monrovia", [Some("Greenwich Mean Time"), Some("GMT"), None, Some("GMT")]),
        (Full, "fr", "Africa/Monrovia", [Some("heure moyenne de Greenwich"), Some("GMT"), None, Some("GMT")]),
        (FullJdk21, "en", "Africa/Monrovia", [Some("Greenwich Mean Time"), Some("GMT"), Some("Greenwich Mean Time"), Some("GMT")]),
        (Full, "de", "Iceland", [Some("Mittlere Greenwich-Zeit"), Some("GMT"), None, Some("GMT")]),
    ];
    let mut m = Mismatches::default();
    for (data, tag, id, want) in cases {
        let (z, l) = (TimeZone::get(id), locale(tag, data));
        for (i, (daylight, style)) in [(false, LONG), (false, SHORT), (true, LONG), (true, SHORT)].into_iter().enumerate() {
            if let Some(want) = want[i] {
                m.check(|| format!("{id} [{tag} {data:?}] name {i}"), &z.display_name(daylight, style, &l), want);
            }
        }
    }
    m.finish("probes");
}
