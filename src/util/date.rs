//! `java.util.Date`.

use crate::lang::JValue;
use crate::text::Locale;

use super::calendar::{DAY_OF_MONTH, DAY_OF_WEEK, DST_OFFSET, HOUR_OF_DAY, MINUTE, MONTH, SECOND, YEAR};
use super::{GregorianCalendar, TimeZone};

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `java.util.Date`: milliseconds since the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(pub i64);

impl Date {
    /// A `java.util.Date` object value.
    pub fn value(millis: i64) -> JValue {
        crate::text::objects::date_value(millis)
    }

    /// `getTime()`: milliseconds since the epoch.
    pub fn time(self) -> i64 {
        self.0
    }

    /// `toString()` in the default time zone (GMT): `Fri Sep 25 07:48:51 GMT 2026`.
    pub fn to_java_string(self) -> String {
        self.to_string_in(&TimeZone::default_zone())
    }

    /// `toString()` with `zone` as the default time zone: `EEE MMM dd HH:mm:ss zzz yyyy` (US names, era year).
    pub fn to_string_in(self, zone: &TimeZone) -> String {
        let mut c = GregorianCalendar::new(self.0, zone, &Locale::us());
        let daylight = c.get(DST_OFFSET) != 0;
        format!(
            "{} {} {:02} {:02}:{:02}:{:02} {} {}",
            DAYS[(c.get(DAY_OF_WEEK) - 1) as usize],
            MONTHS[c.get(MONTH) as usize],
            c.get(DAY_OF_MONTH),
            c.get(HOUR_OF_DAY),
            c.get(MINUTE),
            c.get(SECOND),
            zone.display_name(daylight, super::timezone::SHORT, &Locale::us()),
            c.get(YEAR)
        )
    }

    /// `toGMTString()` with the default time zone (GMT): `25 Sep 2026 07:48:51 GMT`.
    pub fn to_gmt_string(self) -> String {
        self.to_gmt_string_in(&TimeZone::default_zone())
    }

    /// `toGMTString()` with `zone` as the default time zone, which (as in the JDK) moves the Julian/Gregorian
    /// switch by the zone's offset.
    pub fn to_gmt_string_in(self, zone: &TimeZone) -> String {
        use super::calendar::{EPOCH_OFFSET, GREGORIAN_CUTOVER, gregorian_from_fixed, julian_from_fixed};
        let day = 86_400_000i64;
        let fixed = self.0.div_euclid(day) + EPOCH_OFFSET;
        let ms = self.0.rem_euclid(day);
        let gregorian = self.0 >= 0 || self.0 >= GREGORIAN_CUTOVER - zone.offset(self.0) as i64;
        let d = if gregorian { gregorian_from_fixed(fixed) } else { julian_from_fixed(fixed) };
        let year = if d.year <= 0 { 1 - d.year } else { d.year };
        format!("{} {} {} {:02}:{:02}:{:02} GMT", d.day, MONTHS[(d.month - 1) as usize], year, ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60)
    }

    /// `hashCode()`.
    pub fn java_hash_code(self) -> i32 {
        (self.0 as i32) ^ ((self.0 as u64 >> 32) as i32)
    }
}
