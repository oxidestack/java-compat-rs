//! Locale and commons-lang3 LocaleUtils.toLocale against JDK 26 fixtures.

mod common;

use common::*;
use java_compat::text::Locale;

#[test]
fn locales() {
    for (provider, data) in PROVIDERS {
        let mut m = Mismatches::default();
        for c in fixture(provider, "locale").as_array().unwrap() {
            let a: Vec<&str> = c["args"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            let l = Locale::new(a[0], a[1], a[2]).with_data(data);
            let what = format!("{provider} new Locale({a:?})");
            let got = format!("{} {} {} {} {} {} {}", l, l.language(), l.country(), l.variant(), l.java_hash_code(), l.to_language_tag(), l.display_name(&Locale::default_locale().with_data(data)));
            let want = format!(
                "{} {} {} {} {} {} {}",
                c["toString"].as_str().unwrap(), c["language"].as_str().unwrap(), c["country"].as_str().unwrap(), c["variant"].as_str().unwrap(),
                c["hashCode"], c["languageTag"].as_str().unwrap(), c["displayName"].as_str().unwrap()
            );
            m.check(|| what.clone(), &got, &want);
            for n in c["names"].as_array().unwrap() {
                let in_locale = locale(n["in"].as_str().unwrap(), data);
                let got = format!("{}|{}|{}|{}", l.display_language(&in_locale), l.display_country(&in_locale), l.display_variant(&in_locale), l.display_name(&in_locale));
                let want = format!("{}|{}|{}|{}", n["language"].as_str().unwrap(), n["country"].as_str().unwrap(), n["variant"].as_str().unwrap(), n["name"].as_str().unwrap());
                m.check(|| format!("{what} in [{in_locale}]"), &got, &want);
            }
        }
        m.finish(provider);
    }
}

#[test]
fn locale_utils_to_locale() {
    let mut m = Mismatches::default();
    for c in fixture("full", "locale_utils").as_array().unwrap() {
        let input = c["input"].as_str().unwrap();
        let got = match Locale::parse(input) {
            Ok(l) => format!("{l} {}|{}|{}", l.language(), l.country(), l.variant()),
            Err(e) => format!("ERROR {e}"),
        };
        let want = match c["error"].as_str() {
            Some(e) => format!("ERROR {e}"),
            None => format!("{} {}|{}|{}", c["toString"].as_str().unwrap(), c["language"].as_str().unwrap(), c["country"].as_str().unwrap(), c["variant"].as_str().unwrap()),
        };
        m.check(|| format!("toLocale({input:?})"), &got, &want);
    }
    m.finish("LocaleUtils");
}
