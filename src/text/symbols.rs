//! Locale data from the JDK's default CLDR provider for root, `en`, `en_US`, `fr` and `fr_FR`: number symbols
//! and patterns, date symbols and patterns. Under a full JDK, number data of every locale of Keycloak's theme
//! languages comes from `locale_numbers.rs` and date data from `locale_dates.rs`, both recorded from JDK 26 and
//! Keycloak's JDK 21; week data from `locale_dates.rs` in every JDK.

use super::locale::{Lang, Locale, LocaleData};

/// `java.text.DecimalFormatSymbols`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecimalFormatSymbols {
    /// `getZeroDigit()`.
    pub zero_digit: char,
    /// `getGroupingSeparator()`.
    pub grouping_separator: char,
    /// `getDecimalSeparator()`.
    pub decimal_separator: char,
    /// `getPerMill()`.
    pub per_mill: char,
    /// `getPercent()`.
    pub percent: char,
    /// `getDigit()`.
    pub digit: char,
    /// `getPatternSeparator()`.
    pub pattern_separator: char,
    /// `getInfinity()`.
    pub infinity: String,
    /// `getNaN()`.
    pub nan: String,
    /// `getMinusSign()`.
    pub minus_sign: char,
    /// `getCurrencySymbol()`.
    pub currency_symbol: String,
    /// `getInternationalCurrencySymbol()`.
    pub international_currency_symbol: String,
    /// `getMonetaryDecimalSeparator()`.
    pub monetary_decimal_separator: char,
    /// `getMonetaryGroupingSeparator()`.
    pub monetary_grouping_separator: char,
    /// `getExponentSeparator()`.
    pub exponent_separator: String,
    /// The text a pattern's `-` expands to (`getMinusSignText()`, JDK 13+): `minus_sign` plus bidi marks in some
    /// locales (`ar`: U+061C `-`). Used only while it contains `minus_sign`; assigning another `minus_sign`
    /// (`setMinusSign`) makes the text that one character.
    pub minus_sign_text: String,
    /// `getPercentText()`, as `minus_sign_text` for `percent`.
    pub percent_text: String,
    /// `getPerMillText()`, as `minus_sign_text` for `per_mill`.
    pub per_mill_text: String,
}

/// One locale's number symbols as recorded from the JDK (`locale_numbers.rs`).
pub(crate) struct NumberData {
    pub zero: char,
    pub grouping: char,
    pub decimal: char,
    pub per_mill: char,
    pub percent: char,
    pub digit: char,
    pub pattern_separator: char,
    pub infinity: &'static str,
    pub nan: &'static str,
    pub minus: char,
    pub currency_symbol: &'static str,
    pub intl_currency_symbol: &'static str,
    pub monetary_decimal: char,
    pub monetary_grouping: char,
    pub exponent: &'static str,
    pub minus_text: &'static str,
    pub percent_text: &'static str,
    pub per_mill_text: &'static str,
}

/// One locale's `NumberFormat` instance patterns (number, integer, currency, percent) and the fraction digits of
/// its currency instance.
pub(crate) struct Patterns {
    pub patterns: [&'static str; 4],
    pub currency_min_frac: i32,
    pub currency_max_frac: i32,
}

/// The recorded number data of `locale`: its own entry, else its language's (with the country's currency), in
/// the table of its JDK; `None` without `jdk.localedata` or for a language the table lacks.
fn recorded(locale: &Locale) -> Option<(&'static NumberData, &'static Patterns, bool)> {
    use super::locale_numbers::{JDK21, JDK26, PATTERNS, SYMBOLS};
    let table: &[(&str, u16, u8)] = match locale.locale_data() {
        LocaleData::JavaBase => return None,
        LocaleData::Full => &JDK26,
        LocaleData::FullJdk21 => &JDK21,
    };
    let find = |tag: &str| table.binary_search_by(|e| e.0.cmp(tag)).ok().map(|i| &table[i]);
    let (language, country) = (locale.language(), locale.country());
    let (entry, exact) = match find(&format!("{language}_{country}")).filter(|_| !country.is_empty()) {
        Some(e) => (e, true),
        None => (find(language)?, country.is_empty()),
    };
    Some((&SYMBOLS[entry.1 as usize], &PATTERNS[entry.2 as usize], exact))
}

impl DecimalFormatSymbols {
    /// `DecimalFormatSymbols.getInstance(locale)`.
    pub fn new(locale: &Locale) -> DecimalFormatSymbols {
        if let Some((d, _, exact)) = recorded(locale) {
            let (intl, currency) = if exact { (d.intl_currency_symbol, d.currency_symbol) } else { currency(locale) };
            return DecimalFormatSymbols {
                zero_digit: d.zero,
                grouping_separator: d.grouping,
                decimal_separator: d.decimal,
                per_mill: d.per_mill,
                percent: d.percent,
                digit: d.digit,
                pattern_separator: d.pattern_separator,
                infinity: d.infinity.into(),
                nan: d.nan.into(),
                minus_sign: d.minus,
                currency_symbol: currency.into(),
                international_currency_symbol: intl.into(),
                monetary_decimal_separator: d.monetary_decimal,
                monetary_grouping_separator: d.monetary_grouping,
                exponent_separator: d.exponent.into(),
                minus_sign_text: d.minus_text.into(),
                percent_text: d.percent_text.into(),
                per_mill_text: d.per_mill_text.into(),
            };
        }
        let (grouping, decimal) = match locale.lang() {
            Lang::Fr => ('\u{202f}', ','),
            _ => (',', '.'),
        };
        let (intl, currency) = currency(locale);
        DecimalFormatSymbols {
            zero_digit: '0',
            grouping_separator: grouping,
            decimal_separator: decimal,
            per_mill: '‰',
            percent: '%',
            digit: '#',
            pattern_separator: ';',
            infinity: "∞".into(),
            nan: "NaN".into(),
            minus_sign: '-',
            currency_symbol: currency.into(),
            international_currency_symbol: intl.into(),
            monetary_decimal_separator: decimal,
            monetary_grouping_separator: grouping,
            exponent_separator: "E".into(),
            minus_sign_text: "-".into(),
            percent_text: "%".into(),
            per_mill_text: "‰".into(),
        }
    }

    /// `getMinusSignText()`: what a pattern's `-` expands to.
    pub fn minus_text(&self) -> String {
        Self::text(&self.minus_sign_text, self.minus_sign)
    }

    /// `getPercentText()`: what a pattern's `%` expands to.
    pub fn percent_text(&self) -> String {
        Self::text(&self.percent_text, self.percent)
    }

    /// `getPerMillText()`: what a pattern's `‰` expands to.
    pub fn per_mill_text(&self) -> String {
        Self::text(&self.per_mill_text, self.per_mill)
    }

    fn text(text: &str, c: char) -> String {
        if text.contains(c) { text.to_string() } else { c.to_string() }
    }

    /// `setCurrency(currency)` on symbols of `locale` (`DecimalFormatSymbols.getInstance(locale)`): the international
    /// symbol becomes the code, the symbol `currency.getSymbol(locale)`.
    pub fn set_currency(&mut self, currency: &crate::util::Currency, locale: &Locale) {
        self.international_currency_symbol = currency.currency_code().to_string();
        self.currency_symbol = currency.symbol(locale).to_string();
    }

    /// `DecimalFormatSymbols.getInstance(Locale.ROOT)`.
    pub fn root() -> DecimalFormatSymbols {
        DecimalFormatSymbols::new(&Locale::root())
    }
}

/// `(international symbol, symbol)` for the locale's currency, as `DecimalFormatSymbols.initializeCurrency`: the
/// country's currency and its `getSymbol(locale)`, else `XXX` and `¤` (no country, an unknown one, or one without a
/// currency).
pub(crate) fn currency(locale: &Locale) -> (&'static str, &'static str) {
    match crate::util::Currency::for_country(locale.country()) {
        Ok(Some(c)) => (c.currency_code(), c.symbol(locale)),
        _ => ("XXX", "\u{a4}"),
    }
}

/// `(minimum, maximum)` fraction digits of the locale's currency instance, whose pattern has `(min, max)`: the
/// recorded ones, else `NumberFormat.adjustForCurrencyDefaultFractionDigits` with the country's currency.
pub(crate) fn currency_fraction_digits(locale: &Locale, (min, max): (i32, i32)) -> (i32, i32) {
    if let Some((_, p, true)) = recorded(locale) {
        return (p.currency_min_frac, p.currency_max_frac);
    }
    let digits = crate::util::Currency::get_instance(currency(locale).0).map_or(-1, |c| c.default_fraction_digits());
    if digits == -1 {
        (min, max)
    } else if min == max {
        (digits, digits)
    } else {
        (min.min(digits), digits)
    }
}

/// Number patterns: number, integer, currency, percent.
pub(crate) fn number_patterns(locale: &Locale) -> [&'static str; 4] {
    if let Some((_, p, _)) = recorded(locale) {
        return p.patterns;
    }
    match locale.lang() {
        Lang::Root => ["#,##0.###", "#,##0", "¤\u{a0}#,##0.00", "#,##0%"],
        Lang::En => ["#,##0.###", "#,##0", "¤#,##0.00", "#,##0%"],
        Lang::Fr => ["#,##0.###", "#,##0", "#,##0.00\u{a0}¤", "#,##0\u{a0}%"],
    }
}

/// `java.text.DateFormatSymbols`, plus the standalone month names `SimpleDateFormat`'s `L` takes from
/// `Calendar.getDisplayName(MONTH, LONG_STANDALONE | SHORT_STANDALONE, locale)`.
#[derive(Debug, Clone)]
pub struct DateFormatSymbols {
    /// `getEras()`.
    pub eras: [&'static str; 2],
    /// `getMonths()`.
    pub months: [&'static str; 12],
    /// `getShortMonths()`.
    pub short_months: [&'static str; 12],
    /// `LLLL`: the month names standing alone (`январь` where `months` has the genitive `января`).
    pub standalone_months: [&'static str; 12],
    /// `LLL`.
    pub standalone_short_months: [&'static str; 12],
    /// Sunday first.
    pub weekdays: [&'static str; 7],
    /// `getShortWeekdays()`, Sunday first.
    pub short_weekdays: [&'static str; 7],
    /// `getAmPmStrings()`.
    pub am_pm: [&'static str; 2],
}

/// One locale's date symbols as recorded from the JDK (`locale_dates.rs`).
pub(crate) struct DateData {
    pub eras: [&'static str; 2],
    pub months: [&'static str; 12],
    pub short_months: [&'static str; 12],
    pub standalone_months: [&'static str; 12],
    pub standalone_short_months: [&'static str; 12],
    pub weekdays: [&'static str; 7],
    pub short_weekdays: [&'static str; 7],
    pub am_pm: [&'static str; 2],
}

/// One locale's `DateFormat` style patterns as recorded from the JDK: date and time by style (`FULL`..`SHORT`),
/// date-time by `date_style * 4 + time_style`.
pub(crate) struct DatePatterns {
    pub date: [&'static str; 4],
    pub time: [&'static str; 4],
    pub date_time: [&'static str; 16],
}

/// The recorded date data of `locale` (symbols, patterns, zone-name row) in the table of its JDK: its own entry, else
/// its language's, else for a language the JDK has no data for the entry of `xx` (root's symbols and patterns; JDK 21
/// names zones from its English fallback adapter there). `None` without `jdk.localedata`, for root, and for a language
/// the JDK has but the table lacks.
pub(crate) fn recorded_dates(locale: &Locale) -> Option<(&'static DateData, &'static DatePatterns, &'static [u16])> {
    use super::locale_dates::{JDK21, JDK26, LANGUAGES_JDK21, LANGUAGES_JDK26, PATTERNS, SYMBOLS, ZONE_ROWS};
    // A JDK's table: (tag, symbols, patterns, zone row) by tag, and the languages it has data for.
    type Tables = (&'static [(&'static str, u16, u16, u16)], &'static [&'static str]);
    let (table, jdk_languages): Tables = match locale.locale_data() {
        LocaleData::JavaBase => return None,
        LocaleData::Full => (&JDK26, &LANGUAGES_JDK26),
        LocaleData::FullJdk21 => (&JDK21, &LANGUAGES_JDK21),
    };
    let find = |tag: &str| table.binary_search_by(|e| e.0.cmp(tag)).ok().map(|i| &table[i]);
    let (language, country) = (locale.language(), locale.country());
    if language.is_empty() {
        return None;
    }
    let entry = find(&format!("{language}_{country}"))
        .filter(|_| !country.is_empty())
        .or_else(|| find(language))
        .or_else(|| jdk_languages.binary_search(&language).is_err().then(|| find("xx")).flatten())?;
    Some((&SYMBOLS[entry.1 as usize], &PATTERNS[entry.2 as usize], &ZONE_ROWS[entry.3 as usize]))
}

/// Whether this crate formats dates in `locale` exactly as its JDK does: root, English and French always; under a
/// full JDK the languages `locale_dates.rs` carries and those the JDK has no data for; everything under `java.base`
/// (root and English data only). Thai in Thailand is not: the JDK gives it a Buddhist calendar.
pub fn has_date_data(locale: &Locale) -> bool {
    let language = locale.language();
    if language == "th" && locale.country() == "TH" {
        return false;
    }
    matches!(language, "" | "en" | "fr") || !locale.locale_data().is_full() || recorded_dates(locale).is_some()
}

const EN_MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const SHORT_MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const EN_WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const SHORT_WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

impl DateFormatSymbols {
    /// `DateFormatSymbols.getInstance(locale)`.
    pub fn new(locale: &Locale) -> DateFormatSymbols {
        if let Some((d, ..)) = recorded_dates(locale) {
            return DateFormatSymbols {
                eras: d.eras,
                months: d.months,
                short_months: d.short_months,
                standalone_months: d.standalone_months,
                standalone_short_months: d.standalone_short_months,
                weekdays: d.weekdays,
                short_weekdays: d.short_weekdays,
                am_pm: d.am_pm,
            };
        }
        let (eras, months, short_months, weekdays, short_weekdays) = match locale.lang() {
            Lang::Root => (["BCE", "CE"], SHORT_MONTHS, SHORT_MONTHS, SHORT_WEEKDAYS, SHORT_WEEKDAYS),
            Lang::En => (["BC", "AD"], EN_MONTHS, SHORT_MONTHS, EN_WEEKDAYS, SHORT_WEEKDAYS),
            Lang::Fr => (
                ["av. J.-C.", "ap. J.-C."],
                ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
                ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
                ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"],
                ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."],
            ),
        };
        DateFormatSymbols {
            eras,
            months,
            short_months,
            standalone_months: months,
            standalone_short_months: short_months,
            weekdays,
            short_weekdays,
            am_pm: ["AM", "PM"],
        }
    }
}

/// `(firstDayOfWeek, minimalDaysInFirstWeek)` of a `Calendar` for the locale (`Calendar.SUNDAY` is 1): the JDK takes
/// them from the locale's region alone.
pub(crate) fn week_data(locale: &Locale) -> (i32, i32) {
    use super::locale_dates::{WEEKS_JAVABASE, WEEKS_JDK21, WEEKS_JDK26};
    let table: &[(&str, u8, u8)] = match locale.locale_data() {
        LocaleData::JavaBase => &WEEKS_JAVABASE,
        LocaleData::Full => &WEEKS_JDK26,
        LocaleData::FullJdk21 => &WEEKS_JDK21,
    };
    let find = |r: &str| table.binary_search_by(|e| e.0.cmp(r)).ok().map(|i| table[i]);
    let (_, first, min) = find(locale.country()).or_else(|| find("XX")).unwrap_or(("", 1, 1));
    (first as i32, min as i32)
}

/// `DateFormat` styles.
pub const FULL: i32 = 0;
/// `DateFormat.LONG`.
pub const LONG: i32 = 1;
/// `DateFormat.MEDIUM`.
pub const MEDIUM: i32 = 2;
/// `DateFormat.SHORT`.
pub const SHORT: i32 = 3;

/// The date pattern for a style (`FULL`..`SHORT`).
pub(crate) fn date_pattern(style: i32, locale: &Locale) -> &'static str {
    if let Some((_, p, _)) = recorded_dates(locale) {
        return p.date[style as usize];
    }
    let table: [&str; 4] = match locale.lang() {
        Lang::Root => ["y MMMM d, EEEE", "y MMMM d", "y MMM d", "y-MM-dd"],
        Lang::En => ["EEEE, MMMM d, y", "MMMM d, y", "MMM d, y", "M/d/yy"],
        Lang::Fr => ["EEEE d MMMM y", "d MMMM y", "d MMM y", "dd/MM/y"],
    };
    table[style as usize]
}

/// The time pattern for a style.
pub(crate) fn time_pattern(style: i32, locale: &Locale) -> &'static str {
    if let Some((_, p, _)) = recorded_dates(locale) {
        return p.time[style as usize];
    }
    let table: [&str; 4] = match locale.lang() {
        Lang::En => ["h:mm:ss\u{202f}a zzzz", "h:mm:ss\u{202f}a z", "h:mm:ss\u{202f}a", "h:mm\u{202f}a"],
        _ => ["HH:mm:ss zzzz", "HH:mm:ss z", "HH:mm:ss", "HH:mm"],
    };
    table[style as usize]
}

/// Joins a date and a time pattern the way the locale's `DateTimePatterns` do.
pub(crate) fn date_time_pattern(date_style: i32, time_style: i32, date: &str, time: &str, locale: &Locale) -> String {
    if let Some((_, p, _)) = recorded_dates(locale) {
        return p.date_time[(date_style * 4 + time_style) as usize].to_string();
    }
    match locale.lang() {
        Lang::Root => format!("{date} {time}"),
        Lang::Fr if date_style == SHORT || time_style == SHORT => format!("{date} {time}"),
        _ => format!("{date}, {time}"),
    }
}
