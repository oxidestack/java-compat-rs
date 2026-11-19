//! The JObject wrappers: their ClassDefs against `Class.getMethods()` of JDK 26, and their behaviour through
//! both the typed (Velocity) and the untyped (JMustache) invocation paths.

mod common;

use common::*;
use java_compat::lang::JValue;
use java_compat::reflect::{self, JType};
use java_compat::text::objects::{as_locale, date_millis};
use java_compat::text::{DecimalFormat, Locale, NumberFormat, SimpleDateFormat};
use java_compat::util::{Date, GregorianCalendar, TimeZone};

fn values() -> Vec<JValue> {
    let l = Locale::us();
    vec![
        l.value(),
        Date::value(1790322531000),
        TimeZone::get("Europe/Paris").value(),
        GregorianCalendar::new(1790322531000, &TimeZone::gmt(), &l).into_value(),
        NumberFormat::number_instance(&l).into_value(),
        SimpleDateFormat::new("yyyy-MM-dd", &l, &TimeZone::gmt()).unwrap().into_value(),
    ]
}

#[test]
fn class_defs_match_get_methods() {
    let jdk = fixture("full", "reflection");
    let mut m = Mismatches::default();
    for v in values() {
        let def = reflect::class_def(&v).expect("class def");
        let c = jdk.as_array().unwrap().iter().find(|c| c["class"].as_str() == Some(&def.name)).unwrap_or_else(|| panic!("{} not recorded", def.name));
        let mut want: Vec<String> = c["superclasses"].as_array().unwrap().iter().chain(c["interfaces"].as_array().unwrap()).map(|s| s.as_str().unwrap().to_string()).collect();
        let mut got: Vec<String> = reflect::supertypes(&def.name).iter().map(|s| s.to_string()).collect();
        want.sort();
        got.sort();
        m.check(|| format!("{} supertypes", def.name), &format!("{got:?}"), &format!("{want:?}"));
        m.check(|| format!("{} public", def.name), &def.public.to_string(), &c["public"].to_string());
        let jdk_methods: Vec<String> = c["methods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                let params: Vec<&str> = m[1].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
                format!("{} {}.{}({}) static={}", m[2].as_str().unwrap(), m[3].as_str().unwrap(), m[0].as_str().unwrap(), params.join(","), m[4])
            })
            .collect();
        for method in &def.methods {
            let params: Vec<String> = method.params.iter().map(JType::type_name).collect();
            let returns = method.returns.as_ref().map(JType::type_name).unwrap_or_else(|| "void".into());
            let sig = format!("{returns} {}.{}({}) static={}", method.declaring_class, method.name, params.join(","), method.is_static);
            let found = jdk_methods.contains(&sig);
            m.check(|| format!("{} method {sig}", def.name), &found.to_string(), "true");
        }
    }
    m.finish("reflection");
}

fn call(v: &JValue, name: &str, args: &[JValue]) -> JValue {
    let JValue::Object(o) = v else { panic!("object") };
    o.invoke(name, args).unwrap_or_else(|| panic!("no method {name}")).unwrap_or_else(|e| panic!("{name}: {e}")).unwrap_or(JValue::Null)
}

fn call_err(v: &JValue, name: &str, args: &[JValue]) -> String {
    let JValue::Object(o) = v else { panic!("object") };
    o.invoke(name, args).unwrap().err().map(|e| e.to_string()).unwrap_or_default()
}

fn typed(v: &JValue, name: &str, params: &[JType], args: &[JValue]) -> JValue {
    let def = reflect::class_def(v).unwrap();
    let m = def.methods_named(name).find(|m| m.params == params).unwrap_or_else(|| panic!("{name}{params:?}"));
    (m.invoke)(v, args).unwrap().unwrap_or(JValue::Null)
}

#[test]
fn locale_methods() {
    let fr = Locale::france().value();
    assert_eq!(call(&fr, "toString", &[]).to_java_string(), "fr_FR");
    assert_eq!(call(&fr, "getDisplayName", &[]).to_java_string(), "French (France)");
    assert_eq!(call(&fr, "getDisplayName", std::slice::from_ref(&fr)).to_java_string(), "français (France)");
    assert_eq!(typed(&fr, "getDisplayCountry", &[JType::class("java.util.Locale")], &[Locale::us().value()]).to_java_string(), "France");
    assert_eq!(call(&fr, "hashCode", &[]).to_java_string(), "3151512");
    assert_eq!(call(&fr, "equals", &[Locale::new("fr", "FR", "").value()]).to_java_string(), "true");
    assert_eq!(fr.to_java_string(), "fr_FR");
    assert_eq!(as_locale(&fr), Some(Locale::france()));
    // Under java.base only, French names are unavailable.
    let fr_base = Locale::france().with_data(java_compat::text::LocaleData::JavaBase).value();
    assert_eq!(call(&fr_base, "getDisplayName", std::slice::from_ref(&fr_base)).to_java_string(), "French (France)");
}

#[test]
fn date_methods() {
    let d = Date::value(1790322531000);
    assert_eq!(d.to_java_string(), "Fri Sep 25 07:48:51 GMT 2026");
    assert_eq!(call(&d, "getTime", &[]).to_java_string(), "1790322531000");
    let later = Date::value(1790322531001);
    assert_eq!(call(&d, "before", std::slice::from_ref(&later)).to_java_string(), "true");
    assert_eq!(call(&d, "after", std::slice::from_ref(&later)).to_java_string(), "false");
    assert_eq!(call(&d, "compareTo", std::slice::from_ref(&later)).to_java_string(), "-1");
    assert_eq!(call(&d, "hashCode", &[]).to_java_string(), "-678831336");
    assert_eq!(call(&d, "toGMTString", &[]).to_java_string(), "25 Sep 2026 07:48:51 GMT");
    assert_eq!(call(&d, "getYear", &[]).to_java_string(), "126");
    assert_eq!(call(&d, "getDay", &[]).to_java_string(), "5");
    assert_eq!(call_err(&d, "before", &[JValue::Null]), "java.lang.NullPointerException");
    // setTime mutates the shared Date.
    call(&d, "setTime", &[JValue::Long(0)]);
    assert_eq!(date_millis(&d), Some(0));
    assert_eq!(d.to_java_string(), "Thu Jan 01 00:00:00 GMT 1970");
    let copy = call(&d, "clone", &[]);
    call(&copy, "setTime", &[JValue::Long(5)]);
    assert_eq!(date_millis(&d), Some(0));
}

#[test]
fn time_zone_methods() {
    let z = TimeZone::get("America/New_York").value();
    assert_eq!(call(&z, "getID", &[]).to_java_string(), "America/New_York");
    assert_eq!(call(&z, "getDisplayName", &[]).to_java_string(), "Eastern Standard Time");
    assert_eq!(call(&z, "getDisplayName", &[JValue::Bool(true), JValue::Int(0)]).to_java_string(), "EDT");
    assert_eq!(call(&z, "getDisplayName", &[JValue::Bool(true), JValue::Int(1), Locale::france().value()]).to_java_string(), "heure d’été de l’Est nord-américain");
    assert_eq!(call(&z, "getRawOffset", &[]).to_java_string(), "-18000000");
    assert_eq!(call(&z, "getOffset", &[JValue::Long(1790322531000)]).to_java_string(), "-14400000");
    assert_eq!(call(&z, "inDaylightTime", &[Date::value(1790322531000)]).to_java_string(), "true");
    assert_eq!(call(&z, "useDaylightTime", &[]).to_java_string(), "true");
    assert_eq!(call_err(&z, "getDisplayName", &[JValue::Bool(true), JValue::Int(2)]), "java.lang.IllegalArgumentException: Illegal style: 2");
    assert_eq!(TimeZone::gmt().value().to_java_string(), "sun.util.calendar.ZoneInfo[id=\"GMT\",offset=0,dstSavings=0,useDaylight=false,transitions=0,lastRule=null]");
}

#[test]
fn calendar_methods() {
    let c = GregorianCalendar::new(1790322531000, &TimeZone::get("Europe/Paris"), &Locale::france()).into_value();
    assert_eq!(call(&c, "get", &[JValue::Int(1)]).to_java_string(), "2026");
    assert_eq!(call(&c, "get", &[JValue::Int(11)]).to_java_string(), "9");
    assert_eq!(call(&c, "getFirstDayOfWeek", &[]).to_java_string(), "2");
    call(&c, "add", &[JValue::Int(2), JValue::Int(5)]);
    assert_eq!(call(&c, "getTime", &[]).to_java_string(), "Thu Feb 25 08:48:51 GMT 2027");
    call(&c, "set", &[JValue::Int(2027), JValue::Int(1), JValue::Int(31)]);
    assert_eq!(call(&c, "get", &[JValue::Int(5)]).to_java_string(), "3");
    assert_eq!(call(&c, "get", &[JValue::Int(2)]).to_java_string(), "2");
    call(&c, "roll", &[JValue::Int(2), JValue::Bool(false)]);
    assert_eq!(call(&c, "get", &[JValue::Int(2)]).to_java_string(), "1");
    assert_eq!(call(&c, "getActualMaximum", &[JValue::Int(5)]).to_java_string(), "28");
    assert_eq!(call(&c, "isLeapYear", &[JValue::Int(2028)]).to_java_string(), "true");
    assert_eq!(call(&c, "getCalendarType", &[]).to_java_string(), "gregory");
    assert_eq!(call_err(&c, "get", &[JValue::Int(17)]), "java.lang.ArrayIndexOutOfBoundsException: Index 17 out of bounds for length 17");
    call(&c, "setLenient", &[JValue::Bool(false)]);
    call(&c, "set", &[JValue::Int(2), JValue::Int(13)]);
    assert_eq!(call_err(&c, "getTime", &[]), "java.lang.IllegalArgumentException: MONTH");
}

#[test]
fn decimal_format_methods() {
    let f = DecimalFormat::with_locale("#,##0.00", &Locale::us()).unwrap().into_value();
    assert_eq!(call(&f, "format", &[JValue::Double(1234.567)]).to_java_string(), "1,234.57");
    assert_eq!(typed(&f, "format", &[JType::Long], &[JValue::Long(-5)]).to_java_string(), "-5.00");
    assert_eq!(call(&f, "format", &[JValue::BigDecimal("0.125".into())]).to_java_string(), "0.12");
    assert_eq!(format!("{:?}", call(&f, "parse", &[JValue::str("1,234.50")])), "java.lang.Double(1234.5)");
    assert_eq!(call_err(&f, "parse", &[JValue::str("x")]), "java.text.ParseException: Unparseable number: \"x\"");
    call(&f, "setMaximumFractionDigits", &[JValue::Int(0)]);
    assert_eq!(call(&f, "toPattern", &[]).to_java_string(), "#,##0");
    assert_eq!(call(&f, "getRoundingMode", &[]).to_java_string(), "HALF_EVEN");
    let up = java_compat::text::RoundingMode::HalfUp.value();
    call(&f, "setRoundingMode", &[up]);
    assert_eq!(call(&f, "format", &[JValue::Double(2.5)]).to_java_string(), "3");
    assert_eq!(call_err(&f, "format", &[JValue::str("x")]), "java.lang.IllegalArgumentException: Cannot format given Object as a Number");
    assert_eq!(call_err(&f, "applyPattern", &[JValue::str("0.0.0")]), "java.lang.IllegalArgumentException: Multiple decimal separators in pattern \"0.0.0\"");
}

#[test]
fn simple_date_format_methods() {
    let f = SimpleDateFormat::new("EEE, d MMM yyyy HH:mm:ss z", &Locale::us(), &TimeZone::gmt()).unwrap().into_value();
    assert_eq!(call(&f, "format", &[Date::value(1790322531000)]).to_java_string(), "Fri, 25 Sep 2026 07:48:51 GMT");
    assert_eq!(call(&f, "format", &[JValue::Long(0)]).to_java_string(), "Thu, 1 Jan 1970 00:00:00 GMT");
    let parsed = call(&f, "parse", &[JValue::str("Fri, 25 Sep 2026 09:48:51 CEST")]);
    assert_eq!(date_millis(&parsed), Some(1790322531000));
    // Parsing a zone name changed the formatter's zone, as in Java.
    assert_eq!(call(&call(&f, "getTimeZone", &[]), "getID", &[]).to_java_string(), "Europe/Paris");
    assert_eq!(call_err(&f, "parse", &[JValue::str("nope")]), "java.text.ParseException: Unparseable date: \"nope\"");
    call(&f, "setTimeZone", &[TimeZone::get("America/New_York").value()]);
    assert_eq!(call(&f, "format", &[Date::value(1790322531000)]).to_java_string(), "Fri, 25 Sep 2026 03:48:51 EDT");
    assert_eq!(call(&f, "toPattern", &[]).to_java_string(), "EEE, d MMM yyyy HH:mm:ss z");
    assert_eq!(call_err(&f, "format", &[JValue::str("x")]), "java.lang.IllegalArgumentException: Cannot format given Object as a Date");
}
