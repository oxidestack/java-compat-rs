//! `java.util.Currency` and `DecimalFormatSymbols.setCurrency` against the JDKs they were recorded from
//! (`currency.json`): JDK 26 (`full`), the trimmed JDK (`javabase`) and Keycloak 26.5.3's JDK 21 (`jdk21`).

mod common;

use common::*;
use java_compat::lang::JValue;
use java_compat::text::{DecimalFormat, DecimalFormatSymbols, LocaleData, NumberFormat};
use java_compat::util::Currency;
use serde_json::Value;

const LOCALE_PROVIDERS: [(&str, LocaleData); 3] = [
    ("full", LocaleData::Full),
    ("javabase", LocaleData::JavaBase),
    ("jdk21", LocaleData::FullJdk21),
];

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

#[test]
fn codes_and_regions() {
    for (provider, _) in LOCALE_PROVIDERS {
        let f = fixture(provider, "currency");
        let codes = f["codes"].as_array().unwrap();
        let accepted: std::collections::HashSet<&str> = codes.iter().map(|c| s(&c[0])).collect();
        for c in codes {
            let cur = Currency::get_instance(s(&c[0])).unwrap_or_else(|| panic!("{provider}: {c}"));
            assert_eq!(cur.default_fraction_digits() as i64, c[1].as_i64().unwrap(), "{provider}: {c}");
            assert_eq!(cur.numeric_code() as i64, c[2].as_i64().unwrap(), "{provider}: {c}");
        }
        // Every other three-letter code is rejected, as are other spellings.
        for a in b'A'..=b'Z' {
            for b in b'A'..=b'Z' {
                for c in b'A'..=b'Z' {
                    let code = String::from_utf8(vec![a, b, c]).unwrap();
                    assert_eq!(Currency::get_instance(&code).is_some(), accepted.contains(code.as_str()), "{code}");
                }
            }
        }
        for bad in ["usd", "US", "USDX", ""] {
            assert!(Currency::get_instance(bad).is_none(), "{bad}");
        }
        let regions = f["countries"].as_array().unwrap();
        for r in regions {
            let got = Currency::for_country(s(&r[0])).unwrap_or_else(|_| panic!("{provider}: {r}"));
            assert_eq!(got.map(|c| c.currency_code()), r[1].as_str(), "{provider}: {r}");
        }
        assert_eq!(regions.len(), 252);
        assert!(Currency::for_country("ZZ").is_err() && Currency::for_country("").is_err());
    }
}

#[test]
fn symbols_in_every_recorded_locale() {
    let mut checks = 0;
    for (provider, data) in LOCALE_PROVIDERS {
        let f = fixture(provider, "currency");
        let codes: Vec<&str> = f["codes"].as_array().unwrap().iter().map(|c| s(&c[0])).collect();
        for row in f["symbols"].as_array().unwrap() {
            let l = locale(s(&row["locale"]), data);
            for code in &codes {
                let want = row["symbols"].get(*code).map_or(*code, s);
                assert_eq!(Currency::get_instance(code).unwrap().symbol(&l), want, "{provider} {l:?} {code}");
                checks += 1;
            }
        }
    }
    assert!(checks > 150_000, "{checks}");
}

#[test]
fn set_currency_formats() {
    let mut checks = 0;
    for (provider, data) in LOCALE_PROVIDERS {
        let f = fixture(provider, "currency");
        for v in f["formats"].as_array().unwrap() {
            let l = locale(s(&v[0]), data);
            let mut symbols = DecimalFormatSymbols::new(&l);
            symbols.set_currency(&Currency::get_instance(s(&v[1])).unwrap(), &l);
            let df = DecimalFormat::new(s(&v[2]), &symbols).unwrap();
            let got = (
                df.format(&JValue::BigDecimal("-1234.565".into())).unwrap(),
                df.format(&JValue::Double(0.5)).unwrap(),
            );
            assert_eq!((got.0.as_str(), got.1.as_str()), (s(&v[3]), s(&v[4])), "{provider} {v}");
            checks += 1;
        }
    }
    assert!(checks > 10_000, "{checks}");
}

#[test]
fn locales_outside_the_recorded_set() {
    for (provider, data) in LOCALE_PROVIDERS {
        let f = fixture(provider, "currency");
        for v in f["fallbacks"].as_array().unwrap() {
            let l = locale(s(&v["locale"]), data);
            for (code, want) in v["symbols"].as_object().unwrap() {
                assert_eq!(Currency::get_instance(code).unwrap().symbol(&l), s(want), "{provider} {l:?} {code}");
            }
            let d = DecimalFormatSymbols::new(&l);
            assert_eq!(
                (d.international_currency_symbol.as_str(), d.currency_symbol.as_str()),
                (s(&v["intl"]), s(&v["symbol"])),
                "{provider} {l:?}"
            );
            let cf = NumberFormat::currency_instance(&l);
            let want = &v["currencyFormat"];
            assert_eq!(
                (
                    cf.to_pattern(),
                    cf.minimum_fraction_digits() as i64,
                    cf.maximum_fraction_digits() as i64,
                    cf.format(&JValue::Double(-1234.5678)).unwrap()
                ),
                (s(&want[0]).to_string(), want[1].as_i64().unwrap(), want[2].as_i64().unwrap(), s(&want[3]).to_string()),
                "{provider} {l:?}"
            );
        }
    }
}
