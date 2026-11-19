//! `java.util.Locale` (language, country, variant), commons-lang3's `LocaleUtils.toLocale`, and the display
//! names JDK 25's CLDR data gives the languages and countries a template is likely to meet.

use std::fmt;
use std::sync::Arc;

use crate::lang::{JError, JValue, string_hash};

/// A `java.util.Locale` without script or extensions, plus the locale data provider it resolves formatting
/// data from (not part of its identity: equality, hash and `toString` ignore it).
#[derive(Clone, Default)]
pub struct Locale {
    language: Arc<str>,
    country: Arc<str>,
    variant: Arc<str>,
    data: LocaleData,
}

impl PartialEq for Locale {
    fn eq(&self, other: &Locale) -> bool {
        self.language == other.language && self.country == other.country && self.variant == other.variant
    }
}

impl Eq for Locale {}

impl std::hash::Hash for Locale {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (&self.language, &self.country, &self.variant).hash(state);
    }
}

/// Which CLDR locale data the JDK has, which decides how locales other than root and English format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LocaleData {
    /// A full JDK 26 (`jdk.localedata` present): French and the other languages have their own names, symbols
    /// and patterns. This crate carries root, English and French; other languages fall back to root, except for
    /// number symbols and patterns, which it carries for every locale of Keycloak's theme languages
    /// (`locale_numbers.rs`).
    #[default]
    Full,
    /// Only what `java.base` carries (root and English), as in a trimmed runtime such as the MockServer
    /// image: every other language formats with root data (but keeps its country's currency and week data).
    JavaBase,
    /// A full JDK 21 (Keycloak 26's runtime, CLDR 42): as [`LocaleData::Full`], except that number symbols and
    /// patterns come from JDK 21 where they differ from JDK 26 (e.g. `ar` formats with Arabic-Indic digits).
    FullJdk21,
}

impl LocaleData {
    /// Whether the JDK has `jdk.localedata` (CLDR data beyond root and English).
    pub fn is_full(self) -> bool {
        self != LocaleData::JavaBase
    }
}

/// The language data a locale resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    Root,
    En,
    Fr,
}

impl Locale {
    /// `new Locale(language, country, variant)`: the language is lower-cased (and `iw`/`ji`/`in` become
    /// `he`/`yi`/`id`), the country upper-cased, the variant kept as given.
    pub fn new(language: &str, country: &str, variant: &str) -> Locale {
        let language = match language.to_ascii_lowercase().as_str() {
            "iw" => "he".to_string(),
            "ji" => "yi".to_string(),
            "in" => "id".to_string(),
            other => other.to_string(),
        };
        Locale { language: language.into(), country: country.to_ascii_uppercase().into(), variant: variant.into(), data: LocaleData::Full }
    }

    /// The same locale resolving its formatting data from `data`.
    pub fn with_data(mut self, data: LocaleData) -> Locale {
        self.data = data;
        self
    }

    /// The locale data provider this locale resolves from.
    pub fn locale_data(&self) -> LocaleData {
        self.data
    }

    /// `Locale.ROOT`.
    pub fn root() -> Locale {
        Locale::default()
    }

    /// `Locale.ENGLISH`.
    pub fn english() -> Locale {
        Locale::new("en", "", "")
    }

    /// `Locale.US`.
    pub fn us() -> Locale {
        Locale::new("en", "US", "")
    }

    /// `Locale.FRENCH`.
    pub fn french() -> Locale {
        Locale::new("fr", "", "")
    }

    /// `Locale.FRANCE`.
    pub fn france() -> Locale {
        Locale::new("fr", "FR", "")
    }

    /// `Locale.getDefault()` as the ports assume it: `en`, no country. Callers wanting another default pass
    /// their own `Locale` explicitly; there is no global mutable default.
    pub fn default_locale() -> Locale {
        Locale::english()
    }

    /// commons-lang3 (3.20) `LocaleUtils.toLocale(String)`: `en`, `en_US`, `en-US`, `_US`, `en_US_POSIX`, …
    pub fn parse(s: &str) -> Result<Locale, JError> {
        let invalid = || JError::new("java.lang.IllegalArgumentException", format!("Invalid locale format: {s}"));
        if s.is_empty() {
            return Ok(Locale::root());
        }
        if s.contains('#') {
            return Err(invalid());
        }
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 2 {
            return Err(invalid());
        }
        let ch0 = chars[0];
        if ch0 == '_' || ch0 == '-' {
            if chars.len() < 3 || !chars[1].is_uppercase() || !chars[2].is_uppercase() {
                return Err(invalid());
            }
            let country: String = chars[1..3].iter().collect();
            if chars.len() == 3 {
                return Ok(Locale::new("", &country, ""));
            }
            if chars.len() < 5 || chars[3] != ch0 {
                return Err(invalid());
            }
            return Ok(Locale::new("", &country, &chars[4..].iter().collect::<String>()));
        }
        let is_language = |s: &str| !s.is_empty() && s.chars().all(char::is_lowercase) && matches!(s.chars().count(), 2 | 3);
        let is_country = |s: &str| !s.is_empty() && s.chars().all(char::is_uppercase) && s.chars().count() == 2;
        let is_area = |s: &str| s.chars().all(|c| c.is_ascii_digit()) && s.len() == 3;
        if is_language(s) {
            return Ok(Locale::new(s, "", ""));
        }
        let separator = if s.contains('_') { '_' } else { '-' };
        let segments: Vec<&str> = s.splitn(3, separator).collect();
        let language = segments[0];
        match segments.len() {
            2 => {
                let country = segments[1];
                if is_language(language) && is_country(country) || is_area(country) {
                    return Ok(Locale::new(language, country, ""));
                }
            }
            3 => {
                let (country, variant) = (segments[1], segments[2]);
                if is_language(language) && (country.is_empty() || is_country(country) || is_area(country)) && !variant.is_empty() {
                    return Ok(Locale::new(language, country, variant));
                }
            }
            _ => {}
        }
        Err(invalid())
    }

    /// `getLanguage()`.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// `getCountry()`.
    pub fn country(&self) -> &str {
        &self.country
    }

    /// `getVariant()`.
    pub fn variant(&self) -> &str {
        &self.variant
    }

    /// `Locale.hashCode()`.
    pub fn java_hash_code(&self) -> i32 {
        string_hash(&format!("{}{}{}", self.language, self.country.to_ascii_lowercase(), self.variant))
    }

    /// `toLanguageTag()`: `en-US`, `und`, `en-US-POSIX`; variants that are not BCP 47 variant subtags become
    /// private use (`en-US-x-lvariant-FOO`).
    pub fn to_language_tag(&self) -> String {
        let is_variant = |v: &str| {
            v.chars().all(|c| c.is_ascii_alphanumeric()) && ((5..=8).contains(&v.len()) || (v.len() == 4 && v.as_bytes()[0].is_ascii_digit()))
        };
        let subtags: Vec<&str> = self.variant.split(['_', '-']).filter(|v| !v.is_empty()).collect();
        let (variants, private): (Vec<&str>, Vec<&str>) = match subtags.iter().position(|v| !is_variant(v)) {
            Some(i) => (subtags[..i].to_vec(), subtags[i..].to_vec()),
            None => (subtags, Vec::new()),
        };
        let mut parts = Vec::new();
        if !self.language.is_empty() {
            parts.push(self.language.to_string());
        } else if !self.country.is_empty() || !variants.is_empty() || private.is_empty() {
            parts.push("und".to_string());
        }
        if !self.country.is_empty() {
            parts.push(self.country.to_string());
        }
        parts.extend(variants.iter().map(|v| v.to_string()));
        if !private.is_empty() {
            parts.push(format!("x-lvariant-{}", private.join("-")));
        }
        parts.join("-")
    }

    /// The language data this locale resolves to, the way the JDK's CLDR provider falls back.
    pub(crate) fn lang(&self) -> Lang {
        match (&*self.language, self.data) {
            ("en", _) => Lang::En,
            ("fr", data) if data.is_full() => Lang::Fr,
            _ => Lang::Root,
        }
    }

    /// `getDisplayLanguage(inLocale)`.
    pub fn display_language(&self, in_locale: &Locale) -> String {
        if self.language.is_empty() {
            return String::new();
        }
        let french = in_locale.lang() == Lang::Fr;
        LANGUAGES
            .iter()
            .find(|(code, ..)| *code == &*self.language)
            .map(|(_, en, fr)| if french { fr } else { en }.to_string())
            .unwrap_or_else(|| self.language.to_string())
    }

    /// `getDisplayCountry(inLocale)`.
    pub fn display_country(&self, in_locale: &Locale) -> String {
        if self.country.is_empty() {
            return String::new();
        }
        let french = in_locale.lang() == Lang::Fr;
        if let Some((_, en, fr)) = COUNTRIES.iter().find(|(code, ..)| *code == &*self.country) {
            return if french { fr } else { en }.to_string();
        }
        match (&*self.country, in_locale.lang()) {
            ("ZZ", Lang::En) => "Unknown Region".into(),
            ("ZZ", Lang::Fr) => "région inconnue".into(),
            _ => self.country.to_string(),
        }
    }

    /// `getDisplayVariant(inLocale)`: each `_`-separated subtag, comma-separated.
    pub fn display_variant(&self, in_locale: &Locale) -> String {
        self.variant.split('_').map(|v| variant_name(v, in_locale)).collect::<Vec<_>>().join(", ")
    }

    /// `getDisplayName(inLocale)`: `English (United States)`, `français (France)`, `United States`.
    pub fn display_name(&self, in_locale: &Locale) -> String {
        let language = self.display_language(in_locale);
        let mut qualifiers = Vec::new();
        if !self.country.is_empty() {
            qualifiers.push(self.display_country(in_locale));
        }
        if !self.variant.is_empty() {
            qualifiers.push(self.display_variant(in_locale));
        }
        if language.is_empty() {
            if qualifiers.is_empty() {
                return String::new();
            }
            let main = qualifiers.remove(0);
            return if qualifiers.is_empty() { main } else { format!("{main} ({})", qualifiers.join(", ")) };
        }
        if qualifiers.is_empty() { language } else { format!("{language} ({})", qualifiers.join(", ")) }
    }

    /// A `java.util.Locale` object value.
    pub fn value(&self) -> JValue {
        crate::text::objects::locale_value(self.clone())
    }
}

fn variant_name(v: &str, in_locale: &Locale) -> String {
    match (v, in_locale.lang()) {
        ("POSIX", Lang::En) => "Computer".into(),
        ("POSIX", Lang::Fr) => "informatique".into(),
        _ => v.to_string(),
    }
}

impl fmt::Display for Locale {
    /// `Locale.toString()`: `en`, `en_US`, `_US`, `en__POSIX`, `fr_FR`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (l, r, v) = (!self.language.is_empty(), !self.country.is_empty(), !self.variant.is_empty());
        f.write_str(&self.language)?;
        if r || (l && v) {
            write!(f, "_{}", self.country)?;
        }
        if v && (l || r) {
            write!(f, "_{}", self.variant)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Locale({self})")
    }
}

/// Language display names in English and French (JDK 25 CLDR).
const LANGUAGES: &[(&str, &str, &str)] = &[
    ("en", "English", "anglais"),
    ("fr", "French", "français"),
    ("de", "German", "allemand"),
    ("es", "Spanish", "espagnol"),
    ("it", "Italian", "italien"),
    ("pt", "Portuguese", "portugais"),
    ("ja", "Japanese", "japonais"),
    ("zh", "Chinese", "chinois"),
    ("ru", "Russian", "russe"),
    ("nl", "Dutch", "néerlandais"),
    ("he", "Hebrew", "hébreu"),
];

/// Country display names in English and French (JDK 25 CLDR).
const COUNTRIES: &[(&str, &str, &str)] = &[
    ("US", "United States", "États-Unis"),
    ("GB", "United Kingdom", "Royaume-Uni"),
    ("FR", "France", "France"),
    ("DE", "Germany", "Allemagne"),
    ("CA", "Canada", "Canada"),
    ("ES", "Spain", "Espagne"),
    ("MX", "Mexico", "Mexique"),
    ("IT", "Italy", "Italie"),
    ("JP", "Japan", "Japon"),
    ("CN", "China", "Chine"),
    ("BR", "Brazil", "Brésil"),
    ("CH", "Switzerland", "Suisse"),
    ("BE", "Belgium", "Belgique"),
    ("419", "Latin America", "Amérique latine"),
];
