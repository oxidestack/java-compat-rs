//! `JObject` wrappers, with registered `ClassDef`s (typed public methods and supertypes as `Class.getMethods()`
//! shows them in JDK 26), for the `java.util` / `java.text` values templates can see: `Locale`, `Date`,
//! `TimeZone` (`sun.util.calendar.ZoneInfo`), `GregorianCalendar`, `DecimalFormat`, `SimpleDateFormat`.
//!
//! Mutable Java objects (`Date`, `GregorianCalendar`, the formats) share their state between clones of the
//! wrapper, as Java references do; the Java `clone()` method makes a real copy.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::lang::{JError, JObject, JValue, string_hash};
use crate::reflect::{ClassDef, JType, LazyClass};
use crate::util::calendar::{self as cal, GregorianCalendar};
use crate::util::{Date, TimeZone};

use super::date_format::SimpleDateFormat;
use super::decimal::DecimalFormat;
use super::digits::RoundingMode;
use super::locale::Locale;

type R = Result<Option<JValue>, JError>;

fn val(v: JValue) -> R {
    Ok(Some(v))
}

fn void() -> R {
    Ok(None)
}

fn s() -> JType {
    JType::string()
}

fn o() -> JType {
    JType::object()
}

fn c(name: &str) -> JType {
    JType::class(name)
}

const LOCALE: &str = "java.util.Locale";
const DATE: &str = "java.util.Date";
const TIME_ZONE: &str = "java.util.TimeZone";
const ZONE_INFO: &str = "sun.util.calendar.ZoneInfo";
const CALENDAR: &str = "java.util.Calendar";
const GREGORIAN: &str = "java.util.GregorianCalendar";
const FORMAT: &str = "java.text.Format";
const NUMBER_FORMAT: &str = "java.text.NumberFormat";
const DECIMAL_FORMAT: &str = "java.text.DecimalFormat";
const DATE_FORMAT: &str = "java.text.DateFormat";
const SIMPLE_DATE_FORMAT: &str = "java.text.SimpleDateFormat";
const SERIALIZABLE_CLONEABLE: &[&str] = &["java.io.Serializable", "java.lang.Cloneable"];

fn int_arg(v: &JValue) -> i32 {
    match v {
        JValue::Int(i) => *i,
        JValue::Long(l) => *l as i32,
        JValue::Char(c) => *c as i32,
        _ => 0,
    }
}

fn long_arg(v: &JValue) -> i64 {
    match v {
        JValue::Int(i) => *i as i64,
        JValue::Long(l) => *l,
        _ => 0,
    }
}

fn bool_arg(v: &JValue) -> bool {
    matches!(v, JValue::Bool(true))
}

fn npe() -> JError {
    JError::without_message("java.lang.NullPointerException")
}

fn class_cast(v: &JValue, to: &str) -> JError {
    JError::new("java.lang.ClassCastException", format!("class {} cannot be cast to class {to}", v.class_name()))
}

fn string_arg(v: &JValue) -> Result<String, JError> {
    match v {
        JValue::Null => Err(npe()),
        other => Ok(other.to_java_string()),
    }
}

/// The untyped `JObject::invoke` path (JMustache): the first instance method with this name and arity whose
/// parameter types accept the arguments.
fn dispatch(def: &ClassDef, target: &JValue, name: &str, args: &[JValue]) -> Option<R> {
    let accepts = |t: &JType, v: &JValue| match (t, v) {
        (JType::Int, JValue::Int(_) | JValue::Char(_)) => true,
        (JType::Long, JValue::Int(_) | JValue::Long(_)) => true,
        (JType::Double, JValue::Int(_) | JValue::Long(_) | JValue::Float(_) | JValue::Double(_)) => true,
        (JType::Boolean, JValue::Bool(_)) => true,
        (t, _) if t.is_primitive() => false,
        (_, JValue::Null) => true,
        (t, v) => crate::reflect::class_of(v).is_some_and(|actual| t.is_assignable_from(&actual)),
    };
    let method = def.methods_named(name).find(|m| !m.is_static && m.params.len() == args.len() && m.params.iter().zip(args).all(|(t, v)| accepts(t, v)))?;
    Some((method.invoke)(target, args))
}

macro_rules! jobject {
    ($ty:ty, $name:expr, $def:expr, $to_string:expr) => {
        impl JObject for $ty {
            fn class_name(&self) -> String {
                $name.into()
            }

            fn invoke(&self, name: &str, args: &[JValue]) -> Option<R> {
                let def = self.class_def()?;
                // Clones of the wrapper share the Java object's state, so the target can be rebuilt from `self`.
                let target = JValue::Object(Arc::new(self.clone()));
                dispatch(&def, &target, name, args)
            }

            fn to_java_string(&self) -> String {
                ($to_string)(self)
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn class_def(&self) -> Option<Arc<ClassDef>> {
                static DEF: LazyClass = LazyClass::new($def);
                Some(DEF.get())
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------------------
// java.util.Locale

/// A `java.util.Locale` value.
#[derive(Clone)]
pub struct LocaleObject(pub Locale);

pub(crate) fn locale_value(l: Locale) -> JValue {
    JValue::Object(Arc::new(LocaleObject(l)))
}

/// The `Locale` of a `java.util.Locale` value.
pub fn as_locale(v: &JValue) -> Option<Locale> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<LocaleObject>().map(|l| l.0.clone()),
        _ => None,
    }
}

fn locale_arg(v: &JValue) -> Result<Locale, JError> {
    if v.is_null() {
        return Err(npe());
    }
    as_locale(v).ok_or_else(|| class_cast(v, LOCALE))
}

/// The default display locale (`en`) with the provider of `l`.
fn display_locale(l: &Locale) -> Locale {
    Locale::default_locale().with_data(l.locale_data())
}

fn locale_def() -> Arc<ClassDef> {
    let l = || c(LOCALE);
    type L = LocaleObject;
    ClassDef::new(LOCALE)
        .implements(&["java.lang.Cloneable", "java.io.Serializable"])
        .method::<L>("getLanguage", &[], Some(s()), |t, _, _| val(JValue::str(t.0.language())))
        .method::<L>("getCountry", &[], Some(s()), |t, _, _| val(JValue::str(t.0.country())))
        .method::<L>("getVariant", &[], Some(s()), |t, _, _| val(JValue::str(t.0.variant())))
        .method::<L>("getScript", &[], Some(s()), |_, _, _| val(JValue::str("")))
        .method::<L>("getDisplayName", &[], Some(s()), |t, _, _| val(JValue::str(t.0.display_name(&display_locale(&t.0)))))
        .method::<L>("getDisplayName", &[l()], Some(s()), |t, _, a| val(JValue::str(t.0.display_name(&locale_arg(&a[0])?))))
        .method::<L>("getDisplayLanguage", &[], Some(s()), |t, _, _| val(JValue::str(t.0.display_language(&display_locale(&t.0)))))
        .method::<L>("getDisplayLanguage", &[l()], Some(s()), |t, _, a| val(JValue::str(t.0.display_language(&locale_arg(&a[0])?))))
        .method::<L>("getDisplayCountry", &[], Some(s()), |t, _, _| val(JValue::str(t.0.display_country(&display_locale(&t.0)))))
        .method::<L>("getDisplayCountry", &[l()], Some(s()), |t, _, a| val(JValue::str(t.0.display_country(&locale_arg(&a[0])?))))
        .method::<L>("getDisplayVariant", &[], Some(s()), |t, _, _| val(JValue::str(t.0.display_variant(&display_locale(&t.0)))))
        .method::<L>("getDisplayVariant", &[l()], Some(s()), |t, _, a| val(JValue::str(t.0.display_variant(&locale_arg(&a[0])?))))
        .method::<L>("toLanguageTag", &[], Some(s()), |t, _, _| val(JValue::str(t.0.to_language_tag())))
        .method::<L>("hasExtensions", &[], Some(JType::Boolean), |_, _, _| val(JValue::Bool(false)))
        .method::<L>("stripExtensions", &[], Some(l()), |t, _, _| val(t.0.value()))
        .method::<L>("clone", &[], Some(o()), |t, _, _| val(t.0.value()))
        .method::<L>("toString", &[], Some(s()), |t, _, _| val(JValue::str(t.0.to_string())))
        .method::<L>("hashCode", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.0.java_hash_code())))
        .method::<L>("equals", &[o()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(as_locale(&a[0]).is_some_and(|l| l == t.0))))
        .register()
}

jobject!(LocaleObject, LOCALE, locale_def, |t: &LocaleObject| t.0.to_string());

// ---------------------------------------------------------------------------------------------------------
// java.util.Date

/// A `java.util.Date` value (mutable through `setTime`, as in Java).
#[derive(Clone)]
pub struct DateObject(pub Arc<Mutex<i64>>);

impl DateObject {
    /// `getTime()`: milliseconds since the epoch.
    pub fn millis(&self) -> i64 {
        *self.0.lock()
    }
}

pub(crate) fn date_value(millis: i64) -> JValue {
    JValue::Object(Arc::new(DateObject(Arc::new(Mutex::new(millis)))))
}

/// The milliseconds of a `java.util.Date` value.
pub fn date_millis(v: &JValue) -> Option<i64> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<DateObject>().map(DateObject::millis),
        _ => None,
    }
}

fn date_arg(v: &JValue) -> Result<i64, JError> {
    if v.is_null() {
        return Err(npe());
    }
    date_millis(v).ok_or_else(|| class_cast(v, DATE))
}

/// A field of the date in the default time zone (the deprecated `Date` getters).
fn date_field(millis: i64, field: usize) -> i32 {
    GregorianCalendar::new(millis, &TimeZone::default_zone(), &Locale::us()).get(field)
}

fn date_def() -> Arc<ClassDef> {
    let d = || c(DATE);
    let i = || JType::Int;
    type D = DateObject;
    ClassDef::new(DATE)
        .implements(&["java.io.Serializable", "java.lang.Cloneable", "java.lang.Comparable"])
        .method::<D>("getTime", &[], Some(JType::Long), |t, _, _| val(JValue::Long(t.millis())))
        .method::<D>("setTime", &[JType::Long], None, |t, _, a| {
            *t.0.lock() = long_arg(&a[0]);
            void()
        })
        .method::<D>("before", &[d()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(t.millis() < date_arg(&a[0])?)))
        .method::<D>("after", &[d()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(t.millis() > date_arg(&a[0])?)))
        .method::<D>("compareTo", &[d()], Some(i()), |t, _, a| val(JValue::Int(t.millis().cmp(&date_arg(&a[0])?) as i32)))
        .method::<D>("compareTo", &[o()], Some(i()), |t, _, a| val(JValue::Int(t.millis().cmp(&date_arg(&a[0])?) as i32)))
        .method::<D>("equals", &[o()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(date_millis(&a[0]) == Some(t.millis()))))
        .method::<D>("hashCode", &[], Some(i()), |t, _, _| val(JValue::Int(Date(t.millis()).java_hash_code())))
        .method::<D>("toString", &[], Some(s()), |t, _, _| val(JValue::str(Date(t.millis()).to_java_string())))
        .method::<D>("toGMTString", &[], Some(s()), |t, _, _| val(JValue::str(Date(t.millis()).to_gmt_string())))
        .method::<D>("clone", &[], Some(o()), |t, _, _| val(date_value(t.millis())))
        .method::<D>("getYear", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::YEAR) - 1900)))
        .method::<D>("getMonth", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::MONTH))))
        .method::<D>("getDate", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::DATE))))
        .method::<D>("getDay", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::DAY_OF_WEEK) - 1)))
        .method::<D>("getHours", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::HOUR_OF_DAY))))
        .method::<D>("getMinutes", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::MINUTE))))
        .method::<D>("getSeconds", &[], Some(i()), |t, _, _| val(JValue::Int(date_field(t.millis(), cal::SECOND))))
        .method::<D>("getTimezoneOffset", &[], Some(i()), |t, _, _| val(JValue::Int(-(TimeZone::default_zone().offset(t.millis()) / 60_000))))
        .register()
}

jobject!(DateObject, DATE, date_def, |t: &DateObject| Date(t.millis()).to_java_string());

// ---------------------------------------------------------------------------------------------------------
// java.util.TimeZone (sun.util.calendar.ZoneInfo)

/// A `java.util.TimeZone` value (its runtime class is `sun.util.calendar.ZoneInfo`).
#[derive(Clone)]
pub struct TimeZoneObject(pub TimeZone);

impl TimeZone {
    /// A `java.util.TimeZone` object value.
    pub fn value(&self) -> JValue {
        JValue::Object(Arc::new(TimeZoneObject(self.clone())))
    }
}

/// The `TimeZone` of a `java.util.TimeZone` value.
pub fn as_time_zone(v: &JValue) -> Option<TimeZone> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<TimeZoneObject>().map(|z| z.0.clone()),
        _ => None,
    }
}

fn zone_arg(v: &JValue) -> Result<TimeZone, JError> {
    if v.is_null() {
        return Err(npe());
    }
    as_time_zone(v).ok_or_else(|| class_cast(v, TIME_ZONE))
}

/// `ZoneInfo.toString()`: exact for fixed-offset zones; region zones report `transitions=0` (the JDK's count of
/// its compiled transition table is not reproduced).
fn zone_to_string(z: &TimeZone) -> String {
    format!(
        "sun.util.calendar.ZoneInfo[id=\"{}\",offset={},dstSavings={},useDaylight={},transitions=0,lastRule=null]",
        z.id(),
        z.raw_offset(),
        z.dst_savings(),
        z.use_daylight_time()
    )
}

fn display_name(z: &TimeZone, daylight: &JValue, style: &JValue, l: &Locale) -> R {
    let style = int_arg(style);
    if style != crate::util::timezone::SHORT && style != crate::util::timezone::LONG {
        return Err(JError::new("java.lang.IllegalArgumentException", format!("Illegal style: {style}")));
    }
    val(JValue::str(z.display_name(bool_arg(daylight), style, l)))
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn zone_def() -> Arc<ClassDef> {
    ClassDef::new(TIME_ZONE).implements(SERIALIZABLE_CLONEABLE).register();
    let l = || c(LOCALE);
    type Z = TimeZoneObject;
    ClassDef::new(ZONE_INFO)
        .extends(&[TIME_ZONE])
        .implements(SERIALIZABLE_CLONEABLE)
        .method_of::<Z>(TIME_ZONE, "getID", &[], Some(s()), |t, _, _| val(JValue::str(t.0.id())))
        .method_of::<Z>(TIME_ZONE, "getDisplayName", &[], Some(s()), |t, _, _| val(JValue::str(t.0.display_name_long(&Locale::default_locale()))))
        .method_of::<Z>(TIME_ZONE, "getDisplayName", &[l()], Some(s()), |t, _, a| val(JValue::str(t.0.display_name_long(&locale_arg(&a[0])?))))
        .method_of::<Z>(TIME_ZONE, "getDisplayName", &[JType::Boolean, JType::Int], Some(s()), |t, _, a| display_name(&t.0, &a[0], &a[1], &Locale::default_locale()))
        .method_of::<Z>(TIME_ZONE, "getDisplayName", &[JType::Boolean, JType::Int, l()], Some(s()), |t, _, a| display_name(&t.0, &a[0], &a[1], &locale_arg(&a[2])?))
        .method::<Z>("getOffset", &[JType::Long], Some(JType::Int), |t, _, a| val(JValue::Int(t.0.offset(long_arg(&a[0])))))
        .method::<Z>("getRawOffset", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.0.raw_offset())))
        .method::<Z>("getDSTSavings", &[], Some(JType::Int), |t, _, _| val(JValue::Int(t.0.dst_savings())))
        .method::<Z>("useDaylightTime", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.0.use_daylight_time())))
        .method::<Z>("observesDaylightTime", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.0.use_daylight_time() || t.0.in_daylight_time(now()))))
        .method::<Z>("inDaylightTime", &[c(DATE)], Some(JType::Boolean), |t, _, a| val(JValue::Bool(t.0.in_daylight_time(date_arg(&a[0])?))))
        .method::<Z>("hasSameRules", &[c(TIME_ZONE)], Some(JType::Boolean), |t, _, a| {
            let Some(other) = as_time_zone(&a[0]) else { return val(JValue::Bool(false)) };
            let probes = [0i64, 1_790_322_531_000, 1_774_746_000_000, 1_792_890_000_000, -1_000_000_000_000, 4_000_000_000_000];
            val(JValue::Bool(t.0.raw_offset() == other.raw_offset() && probes.iter().all(|p| t.0.offset(*p) == other.offset(*p))))
        })
        .method::<Z>("clone", &[], Some(o()), |t, _, _| val(t.0.value()))
        .method::<Z>("toString", &[], Some(s()), |t, _, _| val(JValue::str(zone_to_string(&t.0))))
        .method::<Z>("hashCode", &[], Some(JType::Int), |t, _, _| val(JValue::Int(string_hash(t.0.id()) ^ t.0.raw_offset())))
        .method::<Z>("equals", &[o()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(as_time_zone(&a[0]).is_some_and(|z| z == t.0))))
        .register()
}

jobject!(TimeZoneObject, ZONE_INFO, zone_def, |t: &TimeZoneObject| zone_to_string(&t.0));

// ---------------------------------------------------------------------------------------------------------
// java.util.GregorianCalendar

/// A `java.util.GregorianCalendar` value (mutable, as in Java).
#[derive(Clone)]
pub struct CalendarObject(pub Arc<Mutex<GregorianCalendar>>);

impl GregorianCalendar {
    /// A `java.util.GregorianCalendar` object value.
    pub fn into_value(self) -> JValue {
        JValue::Object(Arc::new(CalendarObject(Arc::new(Mutex::new(self)))))
    }
}

/// A copy of the calendar of a `java.util.GregorianCalendar` value.
pub fn calendar_of(v: &JValue) -> Option<GregorianCalendar> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<CalendarObject>().map(|c| c.0.lock().clone()),
        _ => None,
    }
}

fn field_arg(v: &JValue) -> Result<usize, JError> {
    let f = int_arg(v);
    if !(0..cal::FIELD_COUNT as i32).contains(&f) {
        return Err(JError::new("java.lang.ArrayIndexOutOfBoundsException", format!("Index {f} out of bounds for length {}", cal::FIELD_COUNT)));
    }
    Ok(f as usize)
}

fn calendar_millis(v: &JValue) -> Option<i64> {
    calendar_of(v).map(|mut c| c.time_in_millis())
}

/// `Calendar.toString()` (abridged: time and zone).
fn calendar_to_string(c: &mut GregorianCalendar) -> String {
    format!("java.util.GregorianCalendar[time={},zone={}]", c.time_in_millis(), zone_to_string(c.time_zone()))
}

fn calendar_def() -> Arc<ClassDef> {
    ClassDef::new(CALENDAR).implements(&["java.io.Serializable", "java.lang.Cloneable", "java.lang.Comparable"]).register();
    let i = || JType::Int;
    type C = CalendarObject;
    ClassDef::new(GREGORIAN)
        .extends(&[CALENDAR])
        .implements(&["java.io.Serializable", "java.lang.Cloneable", "java.lang.Comparable"])
        .method_of::<C>(CALENDAR, "get", &[i()], Some(i()), |t, _, a| {
            let f = field_arg(&a[0])?;
            val(JValue::Int(t.0.lock().checked_get(f)?))
        })
        .method_of::<C>(CALENDAR, "set", &[i(), i()], None, |t, _, a| {
            let f = field_arg(&a[0])?;
            t.0.lock().set(f, int_arg(&a[1]));
            void()
        })
        .method_of::<C>(CALENDAR, "set", &[i(), i(), i()], None, |t, _, a| {
            t.0.lock().set_date(int_arg(&a[0]), int_arg(&a[1]), int_arg(&a[2]));
            void()
        })
        .method_of::<C>(CALENDAR, "set", &[i(), i(), i(), i(), i()], None, |t, _, a| {
            let mut g = t.0.lock();
            g.set_date(int_arg(&a[0]), int_arg(&a[1]), int_arg(&a[2]));
            g.set(cal::HOUR_OF_DAY, int_arg(&a[3]));
            g.set(cal::MINUTE, int_arg(&a[4]));
            void()
        })
        .method_of::<C>(CALENDAR, "set", &[i(), i(), i(), i(), i(), i()], None, |t, _, a| {
            t.0.lock().set_date_time(int_arg(&a[0]), int_arg(&a[1]), int_arg(&a[2]), int_arg(&a[3]), int_arg(&a[4]), int_arg(&a[5]));
            void()
        })
        .method::<C>("add", &[i(), i()], None, |t, _, a| {
            let f = field_arg(&a[0])?;
            t.0.lock().add(f, int_arg(&a[1]))?;
            void()
        })
        .method::<C>("roll", &[i(), JType::Boolean], None, |t, _, a| {
            let f = field_arg(&a[0])?;
            t.0.lock().roll_up(f, bool_arg(&a[1]))?;
            void()
        })
        .method::<C>("roll", &[i(), i()], None, |t, _, a| {
            let f = field_arg(&a[0])?;
            t.0.lock().roll(f, int_arg(&a[1]))?;
            void()
        })
        .method_of::<C>(CALENDAR, "getTime", &[], Some(c(DATE)), |t, _, _| val(date_value(t.0.lock().checked_time_in_millis()?)))
        .method_of::<C>(CALENDAR, "setTime", &[c(DATE)], None, |t, _, a| {
            let m = date_arg(&a[0])?;
            t.0.lock().set_time_in_millis(m);
            void()
        })
        .method_of::<C>(CALENDAR, "getTimeInMillis", &[], Some(JType::Long), |t, _, _| val(JValue::Long(t.0.lock().checked_time_in_millis()?)))
        .method_of::<C>(CALENDAR, "setTimeInMillis", &[JType::Long], None, |t, _, a| {
            t.0.lock().set_time_in_millis(long_arg(&a[0]));
            void()
        })
        .method::<C>("getTimeZone", &[], Some(c(TIME_ZONE)), |t, _, _| val(t.0.lock().time_zone().value()))
        .method::<C>("setTimeZone", &[c(TIME_ZONE)], None, |t, _, a| {
            let z = zone_arg(&a[0])?;
            t.0.lock().set_time_zone(&z);
            void()
        })
        .method_of::<C>(CALENDAR, "isLenient", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.0.lock().is_lenient())))
        .method_of::<C>(CALENDAR, "setLenient", &[JType::Boolean], None, |t, _, a| {
            t.0.lock().set_lenient(bool_arg(&a[0]));
            void()
        })
        .method_of::<C>(CALENDAR, "getFirstDayOfWeek", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().first_day_of_week())))
        .method_of::<C>(CALENDAR, "setFirstDayOfWeek", &[i()], None, |t, _, a| {
            t.0.lock().set_first_day_of_week(int_arg(&a[0]))?;
            void()
        })
        .method_of::<C>(CALENDAR, "getMinimalDaysInFirstWeek", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().minimal_days_in_first_week())))
        .method_of::<C>(CALENDAR, "setMinimalDaysInFirstWeek", &[i()], None, |t, _, a| {
            t.0.lock().set_minimal_days_in_first_week(int_arg(&a[0]))?;
            void()
        })
        .method::<C>("getActualMaximum", &[i()], Some(i()), |t, _, a| val(JValue::Int(t.0.lock().actual_maximum(field_arg(&a[0])?))))
        .method::<C>("getActualMinimum", &[i()], Some(i()), |t, _, a| val(JValue::Int(t.0.lock().actual_minimum(field_arg(&a[0])?))))
        .method::<C>("getMaximum", &[i()], Some(i()), |t, _, a| val(JValue::Int(t.0.lock().maximum(field_arg(&a[0])?))))
        .method::<C>("getMinimum", &[i()], Some(i()), |t, _, a| val(JValue::Int(t.0.lock().minimum(field_arg(&a[0])?))))
        .method::<C>("getGreatestMinimum", &[i()], Some(i()), |t, _, a| val(JValue::Int(t.0.lock().greatest_minimum(field_arg(&a[0])?))))
        .method::<C>("getLeastMaximum", &[i()], Some(i()), |t, _, a| val(JValue::Int(t.0.lock().least_maximum(field_arg(&a[0])?))))
        .method::<C>("isLeapYear", &[i()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(t.0.lock().is_leap_year(int_arg(&a[0])))))
        .method::<C>("getWeekYear", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().week_year())))
        .method::<C>("getWeeksInWeekYear", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().weeks_in_week_year()?)))
        .method::<C>("setWeekDate", &[i(), i(), i()], None, |t, _, a| {
            t.0.lock().set_week_date(int_arg(&a[0]), int_arg(&a[1]), int_arg(&a[2]))?;
            void()
        })
        .method::<C>("isWeekDateSupported", &[], Some(JType::Boolean), |_, _, _| val(JValue::Bool(true)))
        .method::<C>("getCalendarType", &[], Some(s()), |_, _, _| val(JValue::str("gregory")))
        .method_of::<C>(CALENDAR, "clear", &[], None, |t, _, _| {
            t.0.lock().clear();
            void()
        })
        .method_of::<C>(CALENDAR, "clear", &[i()], None, |t, _, a| {
            let f = field_arg(&a[0])?;
            t.0.lock().clear_field(f);
            void()
        })
        .method_of::<C>(CALENDAR, "isSet", &[i()], Some(JType::Boolean), |t, _, a| val(JValue::Bool(t.0.lock().is_set(field_arg(&a[0])?))))
        .method_of::<C>(CALENDAR, "before", &[o()], Some(JType::Boolean), |t, _, a| {
            let mine = t.0.lock().time_in_millis();
            val(JValue::Bool(calendar_millis(&a[0]).is_some_and(|other| mine < other)))
        })
        .method_of::<C>(CALENDAR, "after", &[o()], Some(JType::Boolean), |t, _, a| {
            let mine = t.0.lock().time_in_millis();
            val(JValue::Bool(calendar_millis(&a[0]).is_some_and(|other| mine > other)))
        })
        .method_of::<C>(CALENDAR, "compareTo", &[c(CALENDAR)], Some(i()), |t, _, a| {
            let mine = t.0.lock().time_in_millis();
            let other = calendar_millis(&a[0]).ok_or_else(npe)?;
            val(JValue::Int(mine.cmp(&other) as i32))
        })
        .method::<C>("clone", &[], Some(o()), |t, _, _| val(t.0.lock().clone().into_value()))
        .method::<C>("equals", &[o()], Some(JType::Boolean), |t, _, a| {
            let Some(mut other) = calendar_of(&a[0]) else { return val(JValue::Bool(false)) };
            let mut mine = t.0.lock().clone();
            val(JValue::Bool(mine.java_equals(&mut other)))
        })
        .method::<C>("hashCode", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().java_hash_code())))
        .method_of::<C>(CALENDAR, "toString", &[], Some(s()), |t, _, _| val(JValue::str(calendar_to_string(&mut t.0.lock()))))
        .register()
}

jobject!(CalendarObject, GREGORIAN, calendar_def, |t: &CalendarObject| calendar_to_string(&mut t.0.lock()));

// ---------------------------------------------------------------------------------------------------------
// java.text.DecimalFormat

/// A `java.text.DecimalFormat` value (mutable through its setters).
#[derive(Clone)]
pub struct DecimalFormatObject(pub Arc<Mutex<DecimalFormat>>);

impl DecimalFormat {
    /// A `java.text.DecimalFormat` object value.
    pub fn into_value(self) -> JValue {
        JValue::Object(Arc::new(DecimalFormatObject(Arc::new(Mutex::new(self)))))
    }
}

/// A copy of the format of a `java.text.DecimalFormat` value.
pub fn as_decimal_format(v: &JValue) -> Option<DecimalFormat> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<DecimalFormatObject>().map(|f| f.0.lock().clone()),
        _ => None,
    }
}

/// `Object.toString()`: `class@hash` of the shared state.
fn identity_string<T>(name: &str, state: &Arc<T>) -> String {
    format!("{name}@{:x}", Arc::as_ptr(state) as *const () as usize as u32)
}

/// A `java.math.RoundingMode` constant.
#[derive(Clone)]
pub struct RoundingModeObject(pub RoundingMode);

impl JObject for RoundingModeObject {
    fn class_name(&self) -> String {
        "java.math.RoundingMode".into()
    }
    fn invoke(&self, name: &str, args: &[JValue]) -> Option<R> {
        match (name, args) {
            ("name" | "toString", []) => Some(val(JValue::str(self.0.name()))),
            ("ordinal", []) => Some(val(JValue::Int(self.0 as i32))),
            _ => None,
        }
    }
    fn to_java_string(&self) -> String {
        self.0.name().into()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl RoundingMode {
    /// A `java.math.RoundingMode` object value.
    pub fn value(self) -> JValue {
        JValue::Object(Arc::new(RoundingModeObject(self)))
    }
}

fn rounding_mode_of(v: &JValue) -> Option<RoundingMode> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<RoundingModeObject>().map(|r| r.0),
        JValue::Str(s) => RoundingMode::from_name(s),
        _ => None,
    }
}

fn decimal_def() -> Arc<ClassDef> {
    ClassDef::new(FORMAT).implements(SERIALIZABLE_CLONEABLE).register();
    ClassDef::new(NUMBER_FORMAT).extends(&[FORMAT]).implements(SERIALIZABLE_CLONEABLE).register();
    type D = DecimalFormatObject;
    let i = || JType::Int;
    let b = || JType::Boolean;
    let rounding = || c("java.math.RoundingMode");
    ClassDef::new(DECIMAL_FORMAT)
        .extends(&[NUMBER_FORMAT, FORMAT])
        .implements(SERIALIZABLE_CLONEABLE)
        .method_of::<D>(FORMAT, "format", &[o()], Some(s()), |t, _, a| val(JValue::str(t.0.lock().format(&a[0])?)))
        .method_of::<D>(NUMBER_FORMAT, "format", &[JType::Double], Some(s()), |t, _, a| {
            let d = match &a[0] {
                JValue::Double(d) => *d,
                JValue::Float(f) => *f as f64,
                other => long_arg(other) as f64,
            };
            val(JValue::str(t.0.lock().format_f64(d)?))
        })
        .method_of::<D>(NUMBER_FORMAT, "format", &[JType::Long], Some(s()), |t, _, a| val(JValue::str(t.0.lock().format_i64(long_arg(&a[0]))?)))
        .method_of::<D>(NUMBER_FORMAT, "parse", &[s()], Some(c("java.lang.Number")), |t, _, a| val(t.0.lock().parse_checked(&string_arg(&a[0])?)?))
        .method_of::<D>(FORMAT, "parseObject", &[s()], Some(o()), |t, _, a| val(t.0.lock().parse_checked(&string_arg(&a[0])?)?))
        .method::<D>("toPattern", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().to_pattern())))
        .method::<D>("toLocalizedPattern", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().to_localized_pattern())))
        .method::<D>("applyPattern", &[s()], None, |t, _, a| {
            t.0.lock().apply_pattern(&string_arg(&a[0])?)?;
            void()
        })
        .method::<D>("applyLocalizedPattern", &[s()], None, |t, _, a| {
            t.0.lock().apply_localized_pattern(&string_arg(&a[0])?)?;
            void()
        })
        .method::<D>("getPositivePrefix", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().positive_prefix())))
        .method::<D>("getPositiveSuffix", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().positive_suffix())))
        .method::<D>("getNegativePrefix", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().negative_prefix())))
        .method::<D>("getNegativeSuffix", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().negative_suffix())))
        .method::<D>("setPositivePrefix", &[s()], None, |t, _, a| {
            t.0.lock().set_positive_prefix(&string_arg(&a[0])?);
            void()
        })
        .method::<D>("setPositiveSuffix", &[s()], None, |t, _, a| {
            t.0.lock().set_positive_suffix(&string_arg(&a[0])?);
            void()
        })
        .method::<D>("setNegativePrefix", &[s()], None, |t, _, a| {
            t.0.lock().set_negative_prefix(&string_arg(&a[0])?);
            void()
        })
        .method::<D>("setNegativeSuffix", &[s()], None, |t, _, a| {
            t.0.lock().set_negative_suffix(&string_arg(&a[0])?);
            void()
        })
        .method::<D>("getMultiplier", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().multiplier())))
        .method::<D>("setMultiplier", &[i()], None, |t, _, a| {
            t.0.lock().set_multiplier(int_arg(&a[0]));
            void()
        })
        .method::<D>("getGroupingSize", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().grouping_size())))
        .method::<D>("setGroupingSize", &[i()], None, |t, _, a| {
            t.0.lock().set_grouping_size(int_arg(&a[0]))?;
            void()
        })
        .method_of::<D>(NUMBER_FORMAT, "isGroupingUsed", &[], Some(b()), |t, _, _| val(JValue::Bool(t.0.lock().is_grouping_used())))
        .method::<D>("setGroupingUsed", &[b()], None, |t, _, a| {
            t.0.lock().set_grouping_used(bool_arg(&a[0]));
            void()
        })
        .method::<D>("isDecimalSeparatorAlwaysShown", &[], Some(b()), |t, _, _| val(JValue::Bool(t.0.lock().is_decimal_separator_always_shown())))
        .method::<D>("setDecimalSeparatorAlwaysShown", &[b()], None, |t, _, a| {
            t.0.lock().set_decimal_separator_always_shown(bool_arg(&a[0]));
            void()
        })
        .method::<D>("isParseBigDecimal", &[], Some(b()), |t, _, _| val(JValue::Bool(t.0.lock().is_parse_big_decimal())))
        .method::<D>("setParseBigDecimal", &[b()], None, |t, _, a| {
            t.0.lock().set_parse_big_decimal(bool_arg(&a[0]));
            void()
        })
        .method_of::<D>(NUMBER_FORMAT, "isParseIntegerOnly", &[], Some(b()), |t, _, _| val(JValue::Bool(t.0.lock().is_parse_integer_only())))
        .method_of::<D>(NUMBER_FORMAT, "setParseIntegerOnly", &[b()], None, |t, _, a| {
            t.0.lock().set_parse_integer_only(bool_arg(&a[0]));
            void()
        })
        .method::<D>("getMaximumIntegerDigits", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().maximum_integer_digits())))
        .method::<D>("getMinimumIntegerDigits", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().minimum_integer_digits())))
        .method::<D>("getMaximumFractionDigits", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().maximum_fraction_digits())))
        .method::<D>("getMinimumFractionDigits", &[], Some(i()), |t, _, _| val(JValue::Int(t.0.lock().minimum_fraction_digits())))
        .method::<D>("setMaximumIntegerDigits", &[i()], None, |t, _, a| {
            t.0.lock().set_maximum_integer_digits(int_arg(&a[0]));
            void()
        })
        .method::<D>("setMinimumIntegerDigits", &[i()], None, |t, _, a| {
            t.0.lock().set_minimum_integer_digits(int_arg(&a[0]));
            void()
        })
        .method::<D>("setMaximumFractionDigits", &[i()], None, |t, _, a| {
            t.0.lock().set_maximum_fraction_digits(int_arg(&a[0]));
            void()
        })
        .method::<D>("setMinimumFractionDigits", &[i()], None, |t, _, a| {
            t.0.lock().set_minimum_fraction_digits(int_arg(&a[0]));
            void()
        })
        .method::<D>("getRoundingMode", &[], Some(rounding()), |t, _, _| val(t.0.lock().rounding_mode().value()))
        .method::<D>("setRoundingMode", &[rounding()], None, |t, _, a| {
            let mode = rounding_mode_of(&a[0]).ok_or_else(npe)?;
            t.0.lock().set_rounding_mode(mode);
            void()
        })
        .method::<D>("clone", &[], Some(o()), |t, _, _| val(t.0.lock().clone().into_value()))
        .method::<D>("toString", &[], Some(s()), |t, _, _| val(JValue::str(identity_string(DECIMAL_FORMAT, &t.0))))
        .method::<D>("hashCode", &[], Some(i()), |t, _, _| {
            let f = t.0.lock();
            val(JValue::Int(f.maximum_integer_digits().wrapping_mul(37).wrapping_add(string_hash(f.positive_prefix()))))
        })
        .register()
}

jobject!(DecimalFormatObject, DECIMAL_FORMAT, decimal_def, |t: &DecimalFormatObject| identity_string(DECIMAL_FORMAT, &t.0));

// ---------------------------------------------------------------------------------------------------------
// java.text.SimpleDateFormat

/// A `java.text.SimpleDateFormat` value (mutable, as in Java).
#[derive(Clone)]
pub struct SimpleDateFormatObject(pub Arc<Mutex<SimpleDateFormat>>);

impl SimpleDateFormat {
    /// A `java.text.SimpleDateFormat` object value.
    pub fn into_value(self) -> JValue {
        JValue::Object(Arc::new(SimpleDateFormatObject(Arc::new(Mutex::new(self)))))
    }
}

/// A copy of the format of a `java.text.SimpleDateFormat` value.
pub fn as_simple_date_format(v: &JValue) -> Option<SimpleDateFormat> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<SimpleDateFormatObject>().map(|f| f.0.lock().clone()),
        _ => None,
    }
}

fn simple_date_def() -> Arc<ClassDef> {
    ClassDef::new(FORMAT).implements(SERIALIZABLE_CLONEABLE).register();
    ClassDef::new(DATE_FORMAT).extends(&[FORMAT]).implements(SERIALIZABLE_CLONEABLE).register();
    type F = SimpleDateFormatObject;
    ClassDef::new(SIMPLE_DATE_FORMAT)
        .extends(&[DATE_FORMAT, FORMAT])
        .implements(SERIALIZABLE_CLONEABLE)
        .method_of::<F>(FORMAT, "format", &[o()], Some(s()), |t, _, a| val(JValue::str(t.0.lock().format_value(&a[0])?)))
        .method_of::<F>(DATE_FORMAT, "format", &[c(DATE)], Some(s()), |t, _, a| {
            let m = date_arg(&a[0])?;
            val(JValue::str(t.0.lock().format(m)))
        })
        .method_of::<F>(DATE_FORMAT, "parse", &[s()], Some(c(DATE)), |t, _, a| val(date_value(t.0.lock().parse_checked(&string_arg(&a[0])?)?)))
        .method_of::<F>(FORMAT, "parseObject", &[s()], Some(o()), |t, _, a| val(date_value(t.0.lock().parse_checked(&string_arg(&a[0])?)?)))
        .method::<F>("toPattern", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().to_pattern())))
        .method::<F>("toLocalizedPattern", &[], Some(s()), |t, _, _| val(JValue::str(t.0.lock().to_localized_pattern()?)))
        .method::<F>("applyPattern", &[s()], None, |t, _, a| {
            t.0.lock().apply_pattern(&string_arg(&a[0])?)?;
            void()
        })
        .method_of::<F>(DATE_FORMAT, "getTimeZone", &[], Some(c(TIME_ZONE)), |t, _, _| val(t.0.lock().time_zone().value()))
        .method_of::<F>(DATE_FORMAT, "setTimeZone", &[c(TIME_ZONE)], None, |t, _, a| {
            let z = zone_arg(&a[0])?;
            t.0.lock().set_time_zone(&z);
            void()
        })
        .method_of::<F>(DATE_FORMAT, "isLenient", &[], Some(JType::Boolean), |t, _, _| val(JValue::Bool(t.0.lock().is_lenient())))
        .method_of::<F>(DATE_FORMAT, "setLenient", &[JType::Boolean], None, |t, _, a| {
            t.0.lock().set_lenient(bool_arg(&a[0]));
            void()
        })
        .method::<F>("get2DigitYearStart", &[], Some(c(DATE)), |t, _, _| val(date_value(t.0.lock().two_digit_year_start())))
        .method::<F>("set2DigitYearStart", &[c(DATE)], None, |t, _, a| {
            let m = date_arg(&a[0])?;
            t.0.lock().set_2digit_year_start(m);
            void()
        })
        .method::<F>("clone", &[], Some(o()), |t, _, _| val(t.0.lock().clone().into_value()))
        .method::<F>("hashCode", &[], Some(JType::Int), |t, _, _| val(JValue::Int(string_hash(&t.0.lock().to_pattern()))))
        .method::<F>("toString", &[], Some(s()), |t, _, _| val(JValue::str(identity_string(SIMPLE_DATE_FORMAT, &t.0))))
        .register()
}

jobject!(SimpleDateFormatObject, SIMPLE_DATE_FORMAT, simple_date_def, |t: &SimpleDateFormatObject| identity_string(SIMPLE_DATE_FORMAT, &t.0));
