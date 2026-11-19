# java-compat-rs

<!-- languages:begin -->
**English** · [Русский](README.ru.md) · [中文](README.zh-CN.md) · [Español](README.es.md)
<!-- languages:end -->

An unofficial Rust port of the behaviour of the JDK that other programs need byte for byte: `java.text` and
`java.util` formatting and parsing, `java.util.regex`, `java.util.Calendar`/`TimeZone`/`Currency`, EUC-JP and Big5
charsets, string escaping and a model of Java values. It targets OpenJDK 21 and 26 and is checked against output
recorded from both. Java is a registered trademark of Oracle and/or its affiliates; this project is not affiliated with
or endorsed by Oracle or the OpenJDK project, and it contains no translated OpenJDK source
([`docs/provenance.md`](docs/provenance.md), [`NOTICE`](NOTICE)).

It was written for [mock-server](https://github.com/oxidestack/mock-server), a Rust MockServer that must answer
exactly as the Java original does, and is shared by the other ports of Java libraries in the same family.

Status: pre-release. The crate is not published on crates.io yet; until it is, it is used from a checkout of this
repository.

## Install

The crate is `java-compat-rs` and is used in code as `java_compat`. It is not published on crates.io yet; once it is:

```toml
[dependencies]
java-compat-rs = "0.1"              # not published yet
```

Until then, depend on a checkout: `java-compat-rs = { path = "../java-compat-rs" }`.

## Quick start

```rust
use java_compat::{double_to_string, escape_java};

assert_eq!(double_to_string(0.1 + 0.2), "0.30000000000000004"); // Double.toString
assert_eq!(double_to_string(1e21), "1.0E21");
assert_eq!(escape_java("tab\there \"quoted\""), "tab\\there \\\"quoted\\\"");
```

The API reference will be on docs.rs once the crate is published; `cargo doc --open` builds it from a checkout.

## Guides

[Upgrading](docs/guide/upgrading.md): what each release asks of code that uses the crate. This README and the guides
are also a site, `https://oxidestack.github.io/java-compat-rs/`, once the repository is public.

## Build and test

```sh
cargo test          # unit tests, fixture-driven tests over the recorded JDK output, doc tests
make lint           # clippy, warnings are errors
make deny           # licences, advisories and sources (cargo install cargo-deny --locked)
make doc            # the API documentation, rustdoc's warnings are errors
make public-check   # the publication gate
make site           # the documentation site (site/README.md), in a pinned node container
```

The fixtures in `tests/fixtures/` are committed; recording them needs a JDK (see `reference/` below), running the tests
does not. See [`CONTRIBUTING.md`](CONTRIBUTING.md) to contribute.

## What it covers

Behaviours other ports need byte-for-byte: string escaping, number formatting, a model of Java
values for reflective template engines, and JDK 26's locale-sensitive formatting (`text`, `util`): `Locale`,
`DecimalFormat`/`NumberFormat`, `MessageFormat`/`ChoiceFormat`, `java.util.Formatter`, `SimpleDateFormat`/`DateFormat`,
`Date`, `TimeZone`, `GregorianCalendar`.

JDK 26 is the default everywhere. Where an older runtime differs, a consumer selects it per format rather than
globally: `SimpleDateFormat::set_jdk(Jdk::V21)` (or `with_jdk`) gives JDK 21's lenient parsing, where a pattern
literal must match the text's character exactly, instead of JDK 23+'s matching of any space separator with another
(JDK-8324665; under en_US `h:mm:ss\u202fa`, `"2:54:13 PM"` parses on JDK 26 and not on JDK 21). a consumer
matching Keycloak 26 (OpenJDK 21) selects it; mock-server keeps the JDK 26 default.

`regex` is `java.util.regex` as JDK 21 has it (one port shared by the Rust ports of FreeMarker and Velocity): `Pattern.compile` parses Java's syntax with its `PatternSyntaxException` messages and indices and translates
the pattern into fancy-regex's, spelling out what differs (Java's line terminators for `.`, `^`, `$`, `\Z`; ASCII-only
`\w`/`\d`/`\s`/`\b`, POSIX classes and case folding unless `UNICODE_CHARACTER_CLASS`/`UNICODE_CASE`; Java property
names; `\Q...\E` rewritten first as `RemoveQEQuoting` does; class intersections; inline flags with Java's scoping).
`Matcher.matches`/`lookingAt`/`find`, `replaceAll`/`replaceFirst` with `appendExpandedReplacement`'s syntax and
exceptions, and `Pattern.split(input, limit)` run on the translation; `methods` builds `String.matches`, `replaceAll`,
`replaceFirst` and `split` on them, and `methods::java_regex`/`expand_replacement` expose the translation to
fancy-regex callers. Vectors come from Keycloak 26.5.3's JDK 21 (`tests/fixtures/jdk21/regex.json`,
`reference/run.sh regex`). Known gaps are listed at the top of `src/regex.rs` (Unicode blocks, `\X`, `CANON_EQ`, lone
surrogates).

Number locale data (`DecimalFormatSymbols`, with the multi-character minus, percent and per-mille texts, and the
`NumberFormat` instance patterns) is carried for every locale the JDK has of the languages Keycloak 26.5.3 ships theme
messages for, plus English and French (`src/text/locale_numbers.rs`, generated by `reference/gen_locale_numbers.py`).
`LocaleData::Full` gives JDK 26's; `LocaleData::FullJdk21` gives Keycloak's JDK 21 (CLDR 42), which differs in a few
locales (`ar` formats with Arabic-Indic digits, `de_CH`, `no`/`nb`); `LocaleData::JavaBase` is unchanged. Other
languages still fall back to root.

Date locale data is carried for the same locales the same way (`src/text/locale_dates.rs`, generated by
`reference/gen_locale_dates.py` from JDK 26 and Keycloak's JDK 21): `DateFormatSymbols` (eras, months, weekdays,
AM/PM), the standalone month names `SimpleDateFormat`'s `L` uses, the `DateFormat` style patterns, the display names of
the zones `TimeZone` names, and every region's week data (from each JDK, `java.base` included). `text::has_date_data`
says whether a locale formats dates exactly as its JDK: not for languages the JDK has but the crate does not carry, nor
for `th_TH` (the JDK gives it a Buddhist calendar). Zone names follow a JVM that asked for root's names first (the JDK's
CLDR provider fills missing names into shared arrays, so e.g. fr's short name of `Etc/GMT+5` depends on history).

`util::Currency` is `java.util.Currency` from recorded data (`src/util/currency_data.rs`, generated by
`reference/gen_currency.py` from all three JDKs): the codes `getInstance(String)` accepts (every three-letter code
tried) with their fraction and numeric codes, every region's currency, and `getSymbol(locale)` in the same locales
plus root and an unknown language; `DecimalFormatSymbols` takes its country's currency from it, and
`DecimalFormatSymbols::set_currency` is `setCurrency`. `charset::Charset` is Java's EUC-JP (with JIS X 0212) and Big5
(without HKSCS) encoders, whose WHATWG namesakes in encoding_rs encode differently, recorded char by char
(`src/charset_data.rs`, `reference/gen_charsets.py`; identical in the three JDKs); decoding is not carried.

`reference/` holds the Java programs that record the test fixtures (`tests/fixtures/full` from a full JDK 26,
`tests/fixtures/javabase` from the MockServer image's trimmed JDK 26 with only `java.base` locale data,
`tests/fixtures/jdk21/{locale_numbers,locale_dates,week_data,jdk_languages,currency,charsets}.json` and `regex.json` from the Keycloak
26.5.3 image's JDK 21); rerun `reference/run.sh` to regenerate them (`reference/run.sh localenumbers` or
`reference/run.sh localedates` / `currency` / `charsets` for one set, then the matching `reference/gen_*.py`; `reference/run.sh regex` for the regex vectors).

## Getting help

A question about how the crate works can go to [DeepWiki](https://deepwiki.com/oxidestack/java-compat-rs) first: it
answers from this repository's code and documentation, and where it disagrees with the documentation, the
documentation is right. Questions it does not settle go to
[Discussions](https://github.com/oxidestack/java-compat-rs/discussions), bugs to
[issues](https://github.com/oxidestack/java-compat-rs/issues). <!-- these three links need the public repository; it is not published yet -->

## Licence

The code is licensed under the [Apache License, Version 2.0](LICENSE) (`Apache-2.0`); [`NOTICE`](NOTICE) and
[`docs/provenance.md`](docs/provenance.md) say where its data comes from. The documentation — this README and the
guides under `docs/` — is licensed under the
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) licence
(`CC-BY-4.0`); its legal text is in [`LICENSE-docs`](LICENSE-docs).
