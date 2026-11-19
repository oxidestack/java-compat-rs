//! Number symbols and `NumberFormat` instances of every locale of Keycloak's theme languages (plus English and
//! French) against the JDKs they were recorded from: JDK 26 (`full`), the MockServer image's trimmed JDK (`javabase`)
//! and Keycloak 26.5.3's JDK 21 (`jdk21`, `LocaleData::FullJdk21`). The symbols are the data `locale_numbers.rs` is
//! generated from; the formatted values check that `DecimalFormat` renders with them as the JDK does (multi-character
//! minus and percent texts, Arabic-Indic and Persian digits, `×10^` exponents).

mod common;

use common::*;
use java_compat::text::{DecimalFormat, DecimalFormatSymbols, LocaleData, NumberFormat};
use serde_json::Value;

const LOCALE_PROVIDERS: [(&str, LocaleData); 3] = [
    ("full", LocaleData::Full),
    ("javabase", LocaleData::JavaBase),
    ("jdk21", LocaleData::FullJdk21),
];

fn symbols_line(d: &DecimalFormatSymbols) -> String {
    format!(
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} [{}] [{}] [{}]",
        d.zero_digit,
        d.grouping_separator,
        d.decimal_separator,
        d.per_mill,
        d.percent,
        d.digit,
        d.pattern_separator,
        d.infinity,
        d.nan,
        d.minus_sign,
        d.currency_symbol,
        d.international_currency_symbol,
        d.monetary_decimal_separator,
        d.monetary_grouping_separator,
        d.exponent_separator,
        d.minus_text(),
        d.percent_text(),
        d.per_mill_text()
    )
}

fn expected_symbols_line(s: &Value) -> String {
    let f = |k: &str| s[k].as_str().unwrap().to_string();
    format!(
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} [{}] [{}] [{}]",
        f("zero"),
        f("grouping"),
        f("decimal"),
        f("perMill"),
        f("percent"),
        f("digit"),
        f("patternSeparator"),
        f("infinity"),
        f("nan"),
        f("minus"),
        f("currencySymbol"),
        f("intlCurrencySymbol"),
        f("monetaryDecimal"),
        f("monetaryGrouping"),
        f("exponent"),
        f("minusText"),
        f("percentText"),
        f("perMillText")
    )
}

fn instance(kind: &str, l: &java_compat::text::Locale) -> DecimalFormat {
    match kind {
        "number" => NumberFormat::number_instance(l),
        "integer" => NumberFormat::integer_instance(l),
        "currency" => NumberFormat::currency_instance(l),
        "percent" => NumberFormat::percent_instance(l),
        other => panic!("unknown instance kind {other}"),
    }
}

#[test]
fn locale_number_data() {
    let mut total = 0;
    for (provider, data) in LOCALE_PROVIDERS {
        let mut m = Mismatches::default();
        for s in fixture(provider, "locale_numbers").as_array().unwrap() {
            let tag = s["locale"].as_str().unwrap();
            let l = locale(tag, data);
            m.check(
                || format!("{tag} symbols"),
                &symbols_line(&DecimalFormatSymbols::new(&l)),
                &expected_symbols_line(s),
            );
            for i in s["instances"].as_array().unwrap() {
                let kind = i["kind"].as_str().unwrap();
                let f = instance(kind, &l);
                m.check(
                    || format!("{tag} {kind} pattern"),
                    &f.to_pattern(),
                    i["pattern"].as_str().unwrap(),
                );
                m.check(
                    || format!("{tag} {kind} fraction digits"),
                    &format!(
                        "{}..{}",
                        f.minimum_fraction_digits(),
                        f.maximum_fraction_digits()
                    ),
                    &format!("{}..{}", i["minFrac"], i["maxFrac"]),
                );
                for pair in i["formats"].as_array().unwrap() {
                    let v = value(&pair[0]);
                    let got = f.format(&v).unwrap_or_else(|e| format!("ERROR {e}"));
                    let want = match &pair[1] {
                        Value::String(s) => s.clone(),
                        other => format!("ERROR {}", other["error"].as_str().unwrap()),
                    };
                    m.check(|| format!("{tag} {kind} format {v:?}"), &got, &want);
                }
            }
            total += 1;
        }
        m.finish(&format!("locale number data ({provider})"));
    }
    assert!(total > 600, "{total} locales");
}
