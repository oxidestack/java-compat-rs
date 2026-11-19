//! `java.util.GregorianCalendar` (and the `Calendar` behaviour it inherits).
//!
//! Written from the Java SE API documentation of `Calendar` and `GregorianCalendar` (field resolution, leniency,
//! `add`/`roll` rules, the minimum/maximum table, week numbering, `setWeekDate`) and from behaviour observed in a
//! JDK 21 (probes); no OpenJDK source was consulted.
//!
//! Days are counted as fixed day numbers (day 1 is January 1 of year 1 in the proleptic Gregorian calendar).
//! The calendar is Julian before [`GREGORIAN_CUTOVER`] (October 15, 1582) and Gregorian from it on, so 1582 has
//! 355 days and October 1582 jumps from the 4th to the 15th.

use std::fmt;

use super::TimeZone;
use crate::lang::{JError, string_hash};
use crate::text::Locale;

/// `Calendar.ERA`.
pub const ERA: usize = 0;
/// `Calendar.YEAR`.
pub const YEAR: usize = 1;
/// `Calendar.MONTH`.
pub const MONTH: usize = 2;
/// `Calendar.WEEK_OF_YEAR`.
pub const WEEK_OF_YEAR: usize = 3;
/// `Calendar.WEEK_OF_MONTH`.
pub const WEEK_OF_MONTH: usize = 4;
/// `Calendar.DATE`.
pub const DATE: usize = 5;
/// `Calendar.DAY_OF_MONTH`.
pub const DAY_OF_MONTH: usize = 5;
/// `Calendar.DAY_OF_YEAR`.
pub const DAY_OF_YEAR: usize = 6;
/// `Calendar.DAY_OF_WEEK`.
pub const DAY_OF_WEEK: usize = 7;
/// `Calendar.DAY_OF_WEEK_IN_MONTH`.
pub const DAY_OF_WEEK_IN_MONTH: usize = 8;
/// `Calendar.AM_PM`.
pub const AM_PM: usize = 9;
/// `Calendar.HOUR`.
pub const HOUR: usize = 10;
/// `Calendar.HOUR_OF_DAY`.
pub const HOUR_OF_DAY: usize = 11;
/// `Calendar.MINUTE`.
pub const MINUTE: usize = 12;
/// `Calendar.SECOND`.
pub const SECOND: usize = 13;
/// `Calendar.MILLISECOND`.
pub const MILLISECOND: usize = 14;
/// `Calendar.ZONE_OFFSET`.
pub const ZONE_OFFSET: usize = 15;
/// `Calendar.DST_OFFSET`.
pub const DST_OFFSET: usize = 16;
/// `Calendar.FIELD_COUNT`: the number of fields.
pub const FIELD_COUNT: usize = 17;

/// `GregorianCalendar.BC`.
pub const BCE: i32 = 0;
/// `GregorianCalendar.AD`.
pub const CE: i32 = 1;
/// `Calendar.SUNDAY`.
pub const SUNDAY: i32 = 1;
/// `Calendar.SATURDAY`.
pub const SATURDAY: i32 = 7;

/// The `Calendar` constant names, as `toString()` and the leniency errors print them.
pub const FIELD_NAMES: [&str; FIELD_COUNT] = [
    "ERA",
    "YEAR",
    "MONTH",
    "WEEK_OF_YEAR",
    "WEEK_OF_MONTH",
    "DAY_OF_MONTH",
    "DAY_OF_YEAR",
    "DAY_OF_WEEK",
    "DAY_OF_WEEK_IN_MONTH",
    "AM_PM",
    "HOUR",
    "HOUR_OF_DAY",
    "MINUTE",
    "SECOND",
    "MILLISECOND",
    "ZONE_OFFSET",
    "DST_OFFSET",
];

/// The fixed day number of January 1, 1970.
pub(crate) const EPOCH_OFFSET: i64 = 719_163;

/// The first instant of the Gregorian calendar (October 15, 1582, 00:00 UTC), as `getGregorianChange()` gives it.
pub const GREGORIAN_CUTOVER: i64 = -12_219_292_800_000;

const DAY_MS: i64 = 86_400_000;
/// Month lengths of 1582 (not a leap year in either calendar).
const GREGORIAN_MONTH_DAYS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const HOUR_MS: i64 = 3_600_000;
/// The fixed day number of the first Gregorian day.
const CUTOVER_DAY: i64 = GREGORIAN_CUTOVER / DAY_MS + EPOCH_OFFSET;
const CUTOVER_YEAR: i64 = 1582;
/// The largest `YEAR` the documentation lists for each era.
const MAX_YEAR_CE: i32 = 292_278_994;
const MAX_YEAR_BCE: i32 = 292_269_055;

/// The documented `getMinimum`, `getMaximum`, `getGreatestMinimum` and `getLeastMaximum` values. The least maxima
/// of the week fields depend on the week settings and are computed (see [`GregorianCalendar::least_maximum`]).
const MINIMUM: [i32; FIELD_COUNT] = [
    0,
    1,
    0,
    1,
    0,
    1,
    1,
    1,
    1,
    0,
    0,
    0,
    0,
    0,
    0,
    -13 * 3_600_000,
    0,
];
const MAXIMUM: [i32; FIELD_COUNT] = [
    1,
    MAX_YEAR_CE,
    11,
    53,
    6,
    31,
    366,
    7,
    6,
    1,
    11,
    23,
    59,
    59,
    999,
    14 * 3_600_000,
    2 * 3_600_000,
];
const LEAST_MAXIMUM: [i32; FIELD_COUNT] = [
    1,
    292_269_054,
    11,
    52,
    4,
    28,
    355,
    7,
    3,
    1,
    11,
    23,
    59,
    59,
    999,
    14 * 3_600_000,
    20 * 60_000,
];

/// A year, month (1-12) and day of month, and which calendar they are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CivilDate {
    pub year: i64,
    pub month: i32,
    pub day: i32,
    pub julian: bool,
}

/// Days before the first of `month` (1-12) in a year, without the leap day.
fn days_before_month(month: i32) -> i64 {
    const CUMULATIVE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    CUMULATIVE[(month - 1) as usize]
}

fn gregorian_leap(year: i64) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

fn julian_leap(year: i64) -> bool {
    year.rem_euclid(4) == 0
}

/// The fixed day of a proleptic Gregorian date; `month` is 1-12, `day` may run past the month.
pub(crate) fn gregorian_fixed(year: i64, month: i32, day: i64) -> i64 {
    let y = year - 1;
    let leap_day = i64::from(month > 2 && gregorian_leap(year));
    365 * y + y.div_euclid(4) - y.div_euclid(100)
        + y.div_euclid(400)
        + days_before_month(month)
        + leap_day
        + day
}

/// The fixed day of a proleptic Julian date (astronomical year numbering: year 0 is 1 BC).
pub(crate) fn julian_fixed(year: i64, month: i32, day: i64) -> i64 {
    let y = year - 1;
    let leap_day = i64::from(month > 2 && julian_leap(year));
    // Julian January 1 of year 1 is the Gregorian December 30 of year 0, fixed day -1.
    -2 + 365 * y + y.div_euclid(4) + days_before_month(month) + leap_day + day
}

fn split_year(
    fixed: i64,
    year: i64,
    fixed_of: fn(i64, i32, i64) -> i64,
    julian: bool,
) -> CivilDate {
    // `year` is an estimate within one of the true year.
    let mut year = year;
    while fixed_of(year + 1, 1, 1) <= fixed {
        year += 1;
    }
    while fixed_of(year, 1, 1) > fixed {
        year -= 1;
    }
    let mut month = 12;
    while fixed_of(year, month, 1) > fixed {
        month -= 1;
    }
    CivilDate {
        year,
        month,
        day: (fixed - fixed_of(year, month, 1) + 1) as i32,
        julian,
    }
}

/// The proleptic Gregorian date of a fixed day.
pub(crate) fn gregorian_from_fixed(fd: i64) -> CivilDate {
    // 146097 days in 400 years.
    let estimate =
        (fd - 1).div_euclid(146_097) * 400 + ((fd - 1).rem_euclid(146_097) * 400) / 146_097 + 1;
    split_year(fd, estimate, gregorian_fixed, false)
}

/// The proleptic Julian date of a fixed day.
pub(crate) fn julian_from_fixed(fd: i64) -> CivilDate {
    // 1461 days in 4 years.
    let estimate = (fd + 1).div_euclid(1461) * 4 + ((fd + 1).rem_euclid(1461) * 4) / 1461 + 1;
    split_year(fd, estimate, julian_fixed, true)
}

/// The date of a fixed day in the hybrid calendar: Julian before the cutover, Gregorian from it.
pub(crate) fn civil_from_fixed(fd: i64) -> CivilDate {
    if fd >= CUTOVER_DAY {
        gregorian_from_fixed(fd)
    } else {
        julian_from_fixed(fd)
    }
}

/// `DAY_OF_WEEK` of a fixed day (Sunday is 1).
pub(crate) fn day_of_week(fd: i64) -> i32 {
    // Fixed day 1 is a Monday.
    (fd.rem_euclid(7) + 1) as i32
}

/// The fixed day of a date in the hybrid calendar. `month0` is 0-based and may be any value (it carries into the
/// year); `day` may run past the month. In 1582 a date the Gregorian calendar puts before the cutover is Julian.
fn hybrid_fixed(year: i64, month0: i64, day: i64) -> i64 {
    // The calendar follows the year before months carry into it.
    let named = year;
    let year = year + month0.div_euclid(12);
    let month = (month0.rem_euclid(12) + 1) as i32;
    match named.cmp(&CUTOVER_YEAR) {
        std::cmp::Ordering::Greater => gregorian_fixed(year, month, day),
        std::cmp::Ordering::Less => julian_fixed(year, month, day),
        std::cmp::Ordering::Equal => {
            let g = gregorian_fixed(year, month, day);
            if g >= CUTOVER_DAY {
                g
            } else {
                julian_fixed(year, month, day)
            }
        }
    }
}

fn year_start(year: i64) -> i64 {
    hybrid_fixed(year, 0, 1)
}

fn month_start(year: i64, month0: i64) -> i64 {
    hybrid_fixed(year, month0, 1)
}

/// `(year, 0-based month)` as stored in the fields, from a civil date.
fn era_year(year: i64) -> (i32, i32) {
    if year > 0 {
        (CE, year as i32)
    } else {
        (BCE, (1 - year) as i32)
    }
}

fn cycle(value: i64, amount: i64, min: i64, max: i64) -> i64 {
    let span = max - min + 1;
    (value - min + amount).rem_euclid(span) + min
}

fn illegal(message: impl Into<String>) -> JError {
    JError::new("java.lang.IllegalArgumentException", message)
}

fn illegal_bare() -> JError {
    JError::without_message("java.lang.IllegalArgumentException")
}

/// Stamp of a field that has no value.
const UNSET: u32 = 0;
/// Stamp of a field computed from the time; explicitly set fields count up from [`FIRST_SET`].
const COMPUTED: u32 = 1;
const FIRST_SET: u32 = 2;

/// `java.util.GregorianCalendar` with the default Gregorian change.
#[derive(Clone)]
pub struct GregorianCalendar {
    zone: TimeZone,
    first_day: i32,
    min_days: i32,
    lenient: bool,
    /// The instant; meaningful when `time_valid`.
    time: i64,
    time_valid: bool,
    /// Whether `values` were all computed from `time` in the current zone and week settings.
    fields_valid: bool,
    values: [i32; FIELD_COUNT],
    /// [`UNSET`], [`COMPUTED`], or the order in which the field was set (later is larger).
    stamps: [u32; FIELD_COUNT],
    next_stamp: u32,
}

/// The day-and-week view of one fixed day under given week rules.
struct WeekRules {
    first_day: i32,
    min_days: i32,
}

impl WeekRules {
    /// The first day of week 1 of the period (month or year) that starts on `start`.
    fn week_one(&self, start: i64) -> i64 {
        self.week_one_as(start, day_of_week(start))
    }

    /// [`Self::week_one`] for a period whose first day is taken to fall on `dow`.
    fn week_one_as(&self, start: i64, dow: i32) -> i64 {
        let week_start = start - i64::from(dow - self.first_day).rem_euclid(7);
        let days_in_period = 7 - (start - week_start);
        if days_in_period >= i64::from(self.min_days) {
            week_start
        } else {
            week_start + 7
        }
    }

    /// The week number of `fixed` in the period that starts on `start` (0 or less before week 1).
    fn week_number(&self, fixed: i64, start: i64) -> i64 {
        (fixed - self.week_one(start)).div_euclid(7) + 1
    }

    /// Position of a day of week inside a week (0 for the first day of the week).
    fn index(&self, dow: i64) -> i64 {
        (dow - i64::from(self.first_day)).rem_euclid(7)
    }

    /// `(WEEK_OF_YEAR, week year)` of a day in calendar year `year`.
    fn week_of_year(&self, fixed: i64, year: i64) -> (i64, i64) {
        let week = self.week_number(fixed, year_start(year));
        if week < 1 {
            return (self.week_number(fixed, year_start(year - 1)), year - 1);
        }
        if fixed >= self.week_one(year_start(year + 1)) {
            return (1, year + 1);
        }
        (week, year)
    }

    /// The number of weeks of a week year.
    fn weeks_in(&self, year: i64) -> i64 {
        (self.week_one(year_start(year + 1)) - self.week_one(year_start(year))) / 7
    }

    /// `getWeeksInWeekYear()` of a week year: a week year before the common era counts as the year with the same
    /// number after it.
    fn weeks_of_week_year(&self, week_year: i64) -> i64 {
        self.weeks_in(if week_year > 0 {
            week_year
        } else {
            1 - week_year
        })
    }
}

impl GregorianCalendar {
    /// `new GregorianCalendar(zone, locale)` set to `millis`.
    pub fn new(millis: i64, zone: &TimeZone, locale: &Locale) -> GregorianCalendar {
        let (first_day, min_days) = crate::text::symbols::week_data(locale);
        let mut c = GregorianCalendar {
            zone: zone.clone(),
            first_day,
            min_days,
            lenient: true,
            time: millis,
            time_valid: true,
            fields_valid: false,
            values: [0; FIELD_COUNT],
            stamps: [UNSET; FIELD_COUNT],
            next_stamp: FIRST_SET,
        };
        c.fields_from_time();
        c
    }

    /// `new GregorianCalendar(year, month, dayOfMonth, hourOfDay, minute, second)` in a zone and locale.
    #[allow(clippy::too_many_arguments)]
    pub fn from_fields(
        year: i32,
        month: i32,
        day: i32,
        hour: i32,
        minute: i32,
        second: i32,
        zone: &TimeZone,
        locale: &Locale,
    ) -> GregorianCalendar {
        let mut c = GregorianCalendar::new(0, zone, locale);
        c.clear();
        c.set_date_time(year, month, day, hour, minute, second);
        c
    }

    /// `getTimeZone()`.
    pub fn time_zone(&self) -> &TimeZone {
        &self.zone
    }

    /// `setTimeZone(zone)`: the instant stays, the fields are recomputed in the new zone.
    pub fn set_time_zone(&mut self, zone: &TimeZone) {
        self.zone = zone.clone();
        if self.time_valid {
            self.fields_valid = false;
        }
    }

    /// `getFirstDayOfWeek()`.
    pub fn first_day_of_week(&self) -> i32 {
        self.first_day
    }

    /// `setFirstDayOfWeek(value)`. The new value is kept even when the pending fields cannot be resolved under it
    /// (see `Self::rules_changed`); the error is returned after.
    pub fn set_first_day_of_week(&mut self, v: i32) -> Result<(), JError> {
        if v == self.first_day {
            return Ok(());
        }
        self.refresh_fields();
        self.first_day = v;
        self.rules_changed()
    }

    /// `getMinimalDaysInFirstWeek()`.
    pub fn minimal_days_in_first_week(&self) -> i32 {
        self.min_days
    }

    /// `setMinimalDaysInFirstWeek(value)`, with the same error as [`Self::set_first_day_of_week`].
    pub fn set_minimal_days_in_first_week(&mut self, v: i32) -> Result<(), JError> {
        if v == self.min_days {
            return Ok(());
        }
        self.refresh_fields();
        self.min_days = v;
        self.rules_changed()
    }

    /// After a change of week rules: fields computed from the instant are recomputed, and so are the computed week
    /// fields beside fields set since.
    fn rules_changed(&mut self) -> Result<(), JError> {
        if self.time_valid {
            self.fields_valid = false;
            return Ok(());
        }
        if self.stamps[WEEK_OF_YEAR] != COMPUTED && self.stamps[WEEK_OF_MONTH] != COMPUTED {
            return Ok(());
        }
        // The pending fields resolved (leniently, without the week fields) under the new rules give the new week
        // values. Where they cannot be resolved (only an invalid `ERA` stops a lenient resolution) the JDK throws
        // from the setter, after taking the new value and leaving the fields as they were (observed).
        let mut resolved = self.clone();
        resolved.lenient = true;
        resolved.clear_field(WEEK_OF_YEAR);
        resolved.clear_field(WEEK_OF_MONTH);
        resolved.complete()?;
        for f in [WEEK_OF_YEAR, WEEK_OF_MONTH] {
            if self.stamps[f] == COMPUTED {
                self.values[f] = resolved.values[f];
            }
        }
        Ok(())
    }

    fn rules(&self) -> WeekRules {
        WeekRules {
            first_day: self.first_day,
            min_days: self.min_days,
        }
    }

    /// `isLenient()`.
    pub fn is_lenient(&self) -> bool {
        self.lenient
    }

    /// `setLenient(value)`.
    pub fn set_lenient(&mut self, v: bool) {
        self.lenient = v;
    }

    /// `getTimeInMillis()`; when the fields cannot be resolved (non-lenient), the last valid instant.
    pub fn time_in_millis(&mut self) -> i64 {
        self.checked_time_in_millis().unwrap_or(self.time)
    }

    /// `getTimeInMillis()`, with the `IllegalArgumentException` a non-lenient calendar throws.
    pub fn checked_time_in_millis(&mut self) -> Result<i64, JError> {
        self.complete()?;
        Ok(self.time)
    }

    /// `setTimeInMillis(millis)`.
    pub fn set_time_in_millis(&mut self, millis: i64) {
        self.time = millis;
        self.time_valid = true;
        self.fields_from_time();
    }

    /// `get(field)`; when the fields cannot be resolved (non-lenient), the stored value.
    pub fn get(&mut self, field: usize) -> i32 {
        match self.checked_get(field) {
            Ok(v) => v,
            Err(_) => self.values[field],
        }
    }

    /// `get(field)`, with the exceptions Java throws.
    pub fn checked_get(&mut self, field: usize) -> Result<i32, JError> {
        if field >= FIELD_COUNT {
            return Err(JError::new(
                "java.lang.ArrayIndexOutOfBoundsException",
                format!("Index {field} out of bounds for length {FIELD_COUNT}"),
            ));
        }
        self.complete()?;
        Ok(self.values[field])
    }

    /// `set(field, value)`: recorded now, resolved on the next read.
    pub fn set(&mut self, field: usize, value: i32) {
        if field >= FIELD_COUNT {
            return;
        }
        self.refresh_fields();
        self.values[field] = value;
        self.stamps[field] = self.next_stamp;
        self.next_stamp += 1;
        self.time_valid = false;
        self.fields_valid = false;
    }

    /// `set(year, month, date)`.
    pub fn set_date(&mut self, year: i32, month: i32, date: i32) {
        self.set(YEAR, year);
        self.set(MONTH, month);
        self.set(DAY_OF_MONTH, date);
    }

    /// `set(year, month, date, hourOfDay, minute, second)`.
    pub fn set_date_time(
        &mut self,
        year: i32,
        month: i32,
        date: i32,
        hour_of_day: i32,
        minute: i32,
        second: i32,
    ) {
        self.set_date(year, month, date);
        self.set(HOUR_OF_DAY, hour_of_day);
        self.set(MINUTE, minute);
        self.set(SECOND, second);
    }

    /// `clear()`: every field unset, the instant undefined.
    pub fn clear(&mut self) {
        self.values = [0; FIELD_COUNT];
        self.stamps = [UNSET; FIELD_COUNT];
        self.time_valid = false;
        self.fields_valid = false;
    }

    /// `clear(field)`.
    pub fn clear_field(&mut self, field: usize) {
        if field >= FIELD_COUNT {
            return;
        }
        self.refresh_fields();
        self.values[field] = 0;
        self.stamps[field] = UNSET;
        self.time_valid = false;
        self.fields_valid = false;
    }

    /// `isSet(field)`.
    pub fn is_set(&self, field: usize) -> bool {
        field < FIELD_COUNT && self.stamps[field] != UNSET
    }

    /// Fields computed from the instant before one of them is changed, so the change applies to the current zone
    /// and week rules.
    fn refresh_fields(&mut self) {
        if self.time_valid && !self.fields_valid {
            self.fields_from_time();
        }
    }

    /// `complete()`: resolves set fields into the instant, then recomputes every field from it.
    pub fn complete(&mut self) -> Result<(), JError> {
        if !self.time_valid {
            self.time = self.resolve_time()?;
            self.time_valid = true;
            self.fields_valid = false;
        }
        if !self.fields_valid {
            self.fields_from_time();
        }
        Ok(())
    }

    fn fields_from_time(&mut self) {
        self.values = self.fields_at(self.time);
        self.stamps = [COMPUTED; FIELD_COUNT];
        self.fields_valid = true;
    }

    /// Every field of an instant in this calendar's zone and week rules.
    fn fields_at(&self, time: i64) -> [i32; FIELD_COUNT] {
        let (raw, dst) = self.zone.offsets_at(time);
        self.fields_with_offsets(time, raw, dst)
    }

    /// Every field of an instant seen with the given zone offsets.
    fn fields_with_offsets(&self, time: i64, raw: i32, dst: i32) -> [i32; FIELD_COUNT] {
        let local = time.wrapping_add(i64::from(raw) + i64::from(dst));
        let fixed = local.div_euclid(DAY_MS) + EPOCH_OFFSET;
        let ms = local.rem_euclid(DAY_MS);
        let date = civil_from_fixed(fixed);
        let rules = self.rules();
        let (era, year) = era_year(date.year);
        let month_first = month_start(date.year, i64::from(date.month - 1));
        let (week_of_year, _) = rules.week_of_year(fixed, date.year);
        let hour = (ms / HOUR_MS) as i32;
        let mut v = [0; FIELD_COUNT];
        v[ERA] = era;
        v[YEAR] = year;
        v[MONTH] = date.month - 1;
        v[WEEK_OF_YEAR] = week_of_year as i32;
        v[WEEK_OF_MONTH] = rules.week_number(fixed, month_first) as i32;
        v[DAY_OF_MONTH] = date.day;
        v[DAY_OF_YEAR] = (fixed - year_start(date.year) + 1) as i32;
        v[DAY_OF_WEEK] = day_of_week(fixed);
        v[DAY_OF_WEEK_IN_MONTH] = ((fixed - month_first) / 7 + 1) as i32;
        v[AM_PM] = hour / 12;
        v[HOUR] = hour % 12;
        v[HOUR_OF_DAY] = hour;
        v[MINUTE] = (ms / 60_000 % 60) as i32;
        v[SECOND] = (ms / 1000 % 60) as i32;
        v[MILLISECOND] = (ms % 1000) as i32;
        v[ZONE_OFFSET] = raw;
        v[DST_OFFSET] = dst;
        v
    }

    fn stamp(&self, field: usize) -> u32 {
        self.stamps[field]
    }

    fn has(&self, field: usize) -> bool {
        self.stamps[field] != UNSET
    }

    /// The value of a field, or `default` when it is unset.
    fn value_or(&self, field: usize, default: i32) -> i64 {
        i64::from(if self.has(field) {
            self.values[field]
        } else {
            default
        })
    }

    /// The extended year (1 BC is 0) the `ERA` and `YEAR` fields name.
    fn extended_year(&self) -> Result<i64, JError> {
        let year = self.value_or(YEAR, 1970);
        match self.value_or(ERA, CE) {
            0 => Ok(1 - year),
            1 => Ok(year),
            _ => Err(illegal("Invalid era")),
        }
    }

    /// Which date fields decide the day, by the documented combinations: the complete combination holding the most
    /// recently set field wins (earlier combinations on a tie); when none is complete, the most recently set field
    /// names the combination and the rest take their defaults.
    fn date_combination(&self) -> DateCombination {
        use DateCombination::*;
        let dow = self.stamp(DAY_OF_WEEK);
        // A combination ranks by its most recent field, then by its least recent one.
        let pair = |field: usize| (self.stamp(field).max(dow), self.stamp(field).min(dow));
        let single = |field: usize| (self.stamp(field), self.stamp(field));
        let candidates = [
            (DayOfMonth, self.has(DAY_OF_MONTH), single(DAY_OF_MONTH)),
            (
                WeekOfMonth,
                self.has(WEEK_OF_MONTH) && self.has(DAY_OF_WEEK),
                pair(WEEK_OF_MONTH),
            ),
            (
                DayOfWeekInMonth,
                self.has(DAY_OF_WEEK_IN_MONTH) && self.has(DAY_OF_WEEK),
                pair(DAY_OF_WEEK_IN_MONTH),
            ),
            (DayOfYear, self.has(DAY_OF_YEAR), single(DAY_OF_YEAR)),
            (
                WeekOfYear,
                self.has(WEEK_OF_YEAR) && self.has(DAY_OF_WEEK),
                pair(WEEK_OF_YEAR),
            ),
        ];
        let mut best: Option<(DateCombination, (u32, u32))> = None;
        for (combination, complete, stamp) in candidates {
            if complete && best.is_none_or(|(_, s)| stamp > s) {
                best = Some((combination, stamp));
            }
        }
        if let Some((combination, _)) = best {
            return combination;
        }
        let partial = [
            (WeekOfMonth, self.stamp(WEEK_OF_MONTH)),
            (DayOfWeekInMonth, self.stamp(DAY_OF_WEEK_IN_MONTH)),
            (WeekOfYear, self.stamp(WEEK_OF_YEAR)),
            (DayOfWeekInMonth, dow),
        ];
        let mut best = (DayOfMonth, UNSET);
        for (combination, stamp) in partial {
            if stamp > best.1 {
                best = (combination, stamp);
            }
        }
        best.0
    }

    /// The millisecond of the day the time fields name: `HOUR_OF_DAY`, or `AM_PM` with `HOUR`, whichever was set
    /// more recently (a combination missing one of its fields only when nothing else is set).
    fn time_of_day(&self) -> i64 {
        let hour_of_day = self.has(HOUR_OF_DAY).then(|| self.stamp(HOUR_OF_DAY));
        let twelve =
            (self.has(AM_PM) && self.has(HOUR)).then(|| self.stamp(AM_PM).max(self.stamp(HOUR)));
        let hours = match (hour_of_day, twelve) {
            (Some(h), Some(t)) if t > h => self.value_or(AM_PM, 0) * 12 + self.value_or(HOUR, 0),
            (Some(_), _) => self.value_or(HOUR_OF_DAY, 0),
            _ => self.value_or(AM_PM, 0) * 12 + self.value_or(HOUR, 0),
        };
        let minutes = hours
            .wrapping_mul(60)
            .wrapping_add(self.value_or(MINUTE, 0));
        let seconds = minutes
            .wrapping_mul(60)
            .wrapping_add(self.value_or(SECOND, 0));
        seconds
            .wrapping_mul(1000)
            .wrapping_add(self.value_or(MILLISECOND, 0))
    }

    /// The instant the set fields name.
    fn resolve_time(&self) -> Result<i64, JError> {
        if !self.lenient {
            for f in 0..FIELD_COUNT {
                let v = self.values[f];
                if self.stamps[f] >= FIRST_SET && (v < MINIMUM[f] || v > self.max_for_check(f)) {
                    return Err(illegal(FIELD_NAMES[f]));
                }
            }
        }
        let year = self.extended_year()?;
        let month = self.value_or(MONTH, 0);
        let rules = self.rules();
        let dow = self.value_or(DAY_OF_WEEK, (self.first_day - 1).rem_euclid(7) + 1);
        let combination = self.date_combination();
        // A day counted within a month of 1582 is counted in the Gregorian month when it lands on or after the
        // cutover, otherwise in the Julian month.
        let julian_past_cutover = std::cell::Cell::new(false);
        let in_month = |count: &dyn Fn(i64, i64) -> i64| {
            let (y, m) = (year + month.div_euclid(12), month.rem_euclid(12));
            if year != CUTOVER_YEAR {
                return count(
                    hybrid_fixed(year, month, 1),
                    hybrid_fixed(year, month + 1, 1),
                );
            }
            let gregorian = count(
                gregorian_fixed(y, m as i32 + 1, 1),
                gregorian_fixed(y, m as i32 + 1, 1) + i64::from(GREGORIAN_MONTH_DAYS[m as usize]),
            );
            if gregorian >= CUTOVER_DAY {
                gregorian
            } else {
                let julian = count(
                    julian_fixed(y, m as i32 + 1, 1),
                    julian_fixed(y, m as i32 + 1, 1) + i64::from(GREGORIAN_MONTH_DAYS[m as usize]),
                );
                // A non-lenient calendar rejects a Julian count that lands past the cutover (observed).
                julian_past_cutover.set(julian >= CUTOVER_DAY);
                julian
            }
        };
        let fixed = match combination {
            // In 1582 the calendar is chosen by the day the time of day lands on: Gregorian when the Gregorian
            // reading plus the whole days of the time of day is on or after the cutover (observed).
            DateCombination::DayOfMonth if year == CUTOVER_YEAR => {
                let (y, m) = (year + month.div_euclid(12), month.rem_euclid(12) as i32 + 1);
                let day = self.value_or(DAY_OF_MONTH, 1);
                let g = gregorian_fixed(y, m, day);
                if g + self.time_of_day().div_euclid(DAY_MS) >= CUTOVER_DAY {
                    g
                } else {
                    julian_fixed(y, m, day)
                }
            }
            DateCombination::DayOfMonth => {
                hybrid_fixed(year, month, self.value_or(DAY_OF_MONTH, 1))
            }
            DateCombination::WeekOfMonth => in_month(&|first, _| {
                rules.week_one(first) + (self.value_or(WEEK_OF_MONTH, 1) - 1) * 7 + rules.index(dow)
            }),
            DateCombination::DayOfWeekInMonth => {
                let n = self.value_or(DAY_OF_WEEK_IN_MONTH, 1);
                in_month(&|first, next| {
                    if n >= 0 {
                        first + (dow - i64::from(day_of_week(first))).rem_euclid(7) + (n - 1) * 7
                    } else {
                        let last = next - 1;
                        last - (i64::from(day_of_week(last)) - dow).rem_euclid(7) + (n + 1) * 7
                    }
                })
            }
            DateCombination::DayOfYear => year_start(year) + self.value_or(DAY_OF_YEAR, 1) - 1,
            DateCombination::WeekOfYear => {
                rules.week_one(year_start(year))
                    + (self.value_or(WEEK_OF_YEAR, 1) - 1) * 7
                    + rules.index(dow)
            }
        };
        let local = (fixed - EPOCH_OFFSET)
            .wrapping_mul(DAY_MS)
            .wrapping_add(self.time_of_day());
        let (mut raw, mut dst) =
            if self.stamp(ZONE_OFFSET) >= FIRST_SET && self.stamp(DST_OFFSET) >= FIRST_SET {
                (0, 0)
            } else {
                self.zone.offsets_by_wall(local)
            };
        if self.stamp(ZONE_OFFSET) >= FIRST_SET {
            raw = self.values[ZONE_OFFSET];
        }
        if self.stamp(DST_OFFSET) >= FIRST_SET {
            dst = self.values[DST_OFFSET];
        }
        let time = local.wrapping_sub(i64::from(raw) + i64::from(dst));
        if !self.lenient {
            if julian_past_cutover.get() {
                return Err(illegal("the specified date doesn't exist"));
            }
            if combination == DateCombination::DayOfMonth {
                let y = year + month.div_euclid(12);
                let day = self.value_or(DAY_OF_MONTH, 1);
                if y == CUTOVER_YEAR && month.rem_euclid(12) == 9 && (5..=14).contains(&day) {
                    return Err(illegal("the specified date doesn't exist"));
                }
            }
            // Set zone fields are taken as given, and the others checked as seen with them.
            let (zone_raw, zone_dst) = self.zone.offsets_at(time);
            let seen_raw = if self.stamp(ZONE_OFFSET) >= FIRST_SET {
                raw
            } else {
                zone_raw
            };
            let seen_dst = if self.stamp(DST_OFFSET) >= FIRST_SET {
                dst
            } else {
                zone_dst
            };
            let computed = self.fields_with_offsets(time, seen_raw, seen_dst);
            for f in 0..ZONE_OFFSET {
                if self.stamps[f] >= FIRST_SET && computed[f] != self.values[f] {
                    return Err(illegal(format!(
                        "{}: {} -> {}",
                        FIELD_NAMES[f], self.values[f], computed[f]
                    )));
                }
            }
        }
        Ok(time)
    }

    /// The upper bound a non-lenient calendar accepts for a set field before resolving.
    fn max_for_check(&self, field: usize) -> i32 {
        MAXIMUM[field]
    }

    /// `isLeapYear(year)`: Julian rule up to 1582, Gregorian rule after.
    pub fn is_leap_year(&self, year: i32) -> bool {
        let year = i64::from(year);
        if year > CUTOVER_YEAR {
            gregorian_leap(year)
        } else {
            julian_leap(year)
        }
    }

    /// The fixed day and millisecond of the day of the current fields.
    fn local_day(&self, time: i64) -> i64 {
        let (raw, dst) = self.zone.offsets_at(time);
        time.wrapping_add(i64::from(raw) + i64::from(dst))
            .div_euclid(DAY_MS)
            + EPOCH_OFFSET
    }

    /// The fixed day of the (completed) fields.
    fn current_day(&self) -> i64 {
        self.local_day(self.time)
    }

    fn current_year(&self) -> i64 {
        let year = i64::from(self.values[YEAR]);
        if self.values[ERA] == BCE {
            1 - year
        } else {
            year
        }
    }

    /// Moves to another day, keeping the time of day on the wall clock.
    fn set_day(&mut self, fixed: i64) {
        let date = civil_from_fixed(fixed);
        let (era, year) = era_year(date.year);
        self.set(ERA, era);
        self.set(YEAR, year);
        self.set(MONTH, date.month - 1);
        self.set(DAY_OF_MONTH, date.day);
    }

    /// Sets a year and month, keeping the day of month within the new month (except in 1582, where the JDK lets
    /// a day past the end of the month run into the next one).
    fn set_year_month(&mut self, year: i64, month0: i64) {
        let year = year + month0.div_euclid(12);
        let month0 = month0.rem_euclid(12);
        let last_label = civil_from_fixed(month_start(year, month0 + 1) - 1).day;
        let day = if year == CUTOVER_YEAR {
            self.values[DAY_OF_MONTH]
        } else {
            self.values[DAY_OF_MONTH].min(last_label)
        };
        let (era, y) = era_year(year);
        if era != self.values[ERA] {
            self.set(ERA, era);
        }
        if y != self.values[YEAR] {
            self.set(YEAR, y);
        }
        if month0 as i32 != self.values[MONTH] {
            self.set(MONTH, month0 as i32);
        }
        // The pinned day of month takes its value without being marked as set.
        self.refresh_fields();
        self.values[DAY_OF_MONTH] = day;
        self.time_valid = false;
        self.fields_valid = false;
    }

    /// `add(field, amount)` (Calendar's add rules 1 and 2).
    pub fn add(&mut self, field: usize, amount: i32) -> Result<(), JError> {
        if field >= ZONE_OFFSET {
            return Err(illegal_bare());
        }
        if amount == 0 {
            return Ok(());
        }
        self.complete()?;
        let amount = i64::from(amount);
        let (delta, by_wall) = match field {
            // The year and month fields are set and resolved on the next read, like `set`; the era alone keeps
            // the day of month even where it does not exist (February 29 of a year that is not leap moves on).
            ERA => {
                self.set(
                    ERA,
                    (i64::from(self.values[ERA]) + amount).clamp(0, 1) as i32,
                );
                return Ok(());
            }
            YEAR => {
                self.set_year_month(self.current_year() + amount, i64::from(self.values[MONTH]));
                return Ok(());
            }
            MONTH => {
                self.set_year_month(self.current_year(), i64::from(self.values[MONTH]) + amount);
                return Ok(());
            }
            WEEK_OF_YEAR | WEEK_OF_MONTH | DAY_OF_WEEK_IN_MONTH => (amount * 7 * DAY_MS, true),
            DAY_OF_MONTH | DAY_OF_YEAR | DAY_OF_WEEK => (amount * DAY_MS, true),
            AM_PM => (amount * 12 * HOUR_MS, true),
            HOUR | HOUR_OF_DAY => (amount * HOUR_MS, false),
            MINUTE => (amount * 60_000, false),
            SECOND => (amount * 1000, false),
            _ => (amount, false),
        };
        let before = self.time;
        let mut time = before.wrapping_add(delta);
        if by_wall {
            // Keep the wall-clock time across an offset change, unless that would land on another day than the
            // wall-clock arithmetic names.
            let offset = i64::from(self.zone.offset(before));
            let target = before
                .wrapping_add(offset)
                .wrapping_add(delta)
                .div_euclid(DAY_MS)
                + EPOCH_OFFSET;
            let shift = offset - i64::from(self.zone.offset(time));
            if shift != 0 {
                let shifted = time.wrapping_add(shift);
                if self.local_day(shifted) == target {
                    time = shifted;
                }
            }
        }
        self.set_time_in_millis(time);
        Ok(())
    }

    /// `roll(field, up)`.
    pub fn roll_up(&mut self, field: usize, up: bool) -> Result<(), JError> {
        self.roll(field, if up { 1 } else { -1 })
    }

    /// `roll(field, amount)`: adds to one field without changing larger fields. The new value is set like `set`
    /// does (resolved on the next read): the rolled field itself, or the day of month where the roll is measured in
    /// days of the month.
    pub fn roll(&mut self, field: usize, amount: i32) -> Result<(), JError> {
        if field >= ZONE_OFFSET {
            return Err(illegal_bare());
        }
        if amount == 0 {
            return Ok(());
        }
        self.complete()?;
        let amount = i64::from(amount);
        let value = i64::from(self.values[field]);
        let rules = self.rules();
        let fixed = self.current_day();
        let year = self.current_year();
        let month = i64::from(self.values[MONTH]);
        let dow = i64::from(self.values[DAY_OF_WEEK]);
        // In the year of the Gregorian change the day fields roll over the days that exist, and the new day is set
        // as a date (observed).
        if year == CUTOVER_YEAR && matches!(field, DAY_OF_MONTH | DAY_OF_YEAR | DAY_OF_WEEK) {
            let (first, last) = match field {
                DAY_OF_MONTH => (month_start(year, month), month_start(year, month + 1) - 1),
                DAY_OF_YEAR => (year_start(year), year_start(year + 1) - 1),
                _ => {
                    let week_start = fixed - rules.index(dow);
                    (week_start, week_start + 6)
                }
            };
            self.set_day(first + cycle(fixed - first, amount, 0, last - first));
            return Ok(());
        }
        let new = match field {
            ERA => cycle(value, amount, 0, 1),
            // The day of month stays even where the new year lacks it (February 29 moves on), unlike `add`.
            YEAR => cycle(value, amount, 1, i64::from(MAXIMUM[YEAR])),
            MONTH => {
                self.set_year_month(year, cycle(value, amount, 0, 11));
                return Ok(());
            }
            DAY_OF_MONTH => cycle(
                value,
                amount,
                1,
                i64::from(civil_from_fixed(month_start(year, month + 1) - 1).day),
            ),
            DAY_OF_YEAR => cycle(value, amount, 1, year_start(year + 1) - year_start(year)),
            DAY_OF_WEEK => {
                // Within the current week: set as a week of the year, or as a date in the first and last weeks.
                let index = rules.index(dow);
                let week_start = fixed - index;
                let day = week_start + cycle(index, amount, 0, 6);
                // Weeks numbered 1 or 52 and above (whatever the year's week count) count as dates (observed).
                let week = self.values[WEEK_OF_YEAR];
                if week <= 1 || week >= 52 {
                    self.set_day(day);
                    return Ok(());
                }
                // The day of month keeps its old value: clearing `DAY_OF_WEEK` afterwards goes back to the old day
                // (observed).
                self.set(WEEK_OF_YEAR, self.values[WEEK_OF_YEAR]);
                i64::from(day_of_week(day))
            }
            WEEK_OF_MONTH => {
                // Weeks 1 to the last week of the month; the day is kept inside the month.
                let first = month_start(year, month);
                let last = month_start(year, month + 1) - 1;
                let week = cycle(
                    value,
                    amount,
                    1,
                    i64::from(self.actual_maximum(WEEK_OF_MONTH)),
                );
                let day =
                    (rules.week_one(first) + (week - 1) * 7 + rules.index(dow)).clamp(first, last);
                self.set(DAY_OF_MONTH, civil_from_fixed(day).day);
                return Ok(());
            }
            DAY_OF_WEEK_IN_MONTH => {
                let first = month_start(year, month);
                let last = month_start(year, month + 1) - 1;
                let first_same = first + (dow - i64::from(day_of_week(first))).rem_euclid(7);
                let count = (last - first_same) / 7 + 1;
                let current = (fixed - first_same) / 7 + 1;
                let new = cycle(current, amount, 1, count);
                // In the year of the Gregorian change the JDK moves the day of month instead (observed).
                if year == CUTOVER_YEAR {
                    self.set(
                        DAY_OF_MONTH,
                        civil_from_fixed(fixed + (new - current) * 7).day,
                    );
                    return Ok(());
                }
                self.set(DAY_OF_WEEK, dow as i32);
                new
            }
            WEEK_OF_YEAR => {
                // Through the weeks of the calendar year whose day of the current weekday lies in the year. A day in
                // a week of the neighbouring week year counts from the edge week instead (all observed): from the
                // last week when the week year is the next one (downwards one step less; upwards at least to the
                // first week that has the weekday in the year), from week 1 when it is the previous one.
                let start = year_start(year);
                let end = year_start(year + 1) - 1;
                let weeks = i64::from(self.actual_maximum(WEEK_OF_YEAR));
                let in_year = |week: i64| {
                    (start..=end)
                        .contains(&(rules.week_one(start) + (week - 1) * 7 + rules.index(dow)))
                };
                let low = if in_year(1) { 1 } else { 2 };
                let (_, week_year) = rules.week_of_year(fixed, year);
                match week_year.cmp(&year) {
                    std::cmp::Ordering::Equal => {
                        let high = if in_year(weeks) { weeks } else { weeks - 1 };
                        cycle(value, amount, low, high)
                    }
                    std::cmp::Ordering::Greater if amount < 0 => cycle(weeks, amount + 1, 1, weeks),
                    std::cmp::Ordering::Greater => cycle(weeks, amount, 1, weeks).max(low),
                    // Upwards from week 1 less the weeks the previous week year has beyond this year's last.
                    std::cmp::Ordering::Less if amount > 0 => {
                        cycle(1, amount - (value - weeks), 1, weeks)
                    }
                    std::cmp::Ordering::Less => cycle(1, amount, 1, weeks),
                }
            }
            AM_PM => cycle(value, amount, 0, 1),
            // The rest of the time of day is resolved at once (on the wall clock). Rolling the hour by a whole
            // number of cycles moves it by one, as in the JDK.
            _ => {
                let span = i64::from(MAXIMUM[field]) + 1;
                let hours = matches!(field, HOUR | HOUR_OF_DAY);
                let amount = if hours && amount % span == 0 {
                    amount.signum()
                } else {
                    amount
                };
                self.set(field, cycle(value, amount, 0, span - 1) as i32);
                return self.complete();
            }
        };
        if field == WEEK_OF_YEAR && year == CUTOVER_YEAR {
            // In the year of the Gregorian change the new week's day is set as a date (observed).
            self.set_day(rules.week_one(year_start(year)) + (new - 1) * 7 + rules.index(dow));
            return Ok(());
        }
        self.set(field, new as i32);
        Ok(())
    }

    /// `getMinimum(field)`.
    pub fn minimum(&self, field: usize) -> i32 {
        MINIMUM[field]
    }

    /// `getMaximum(field)`.
    pub fn maximum(&self, field: usize) -> i32 {
        MAXIMUM[field]
    }

    /// `getGreatestMinimum(field)`: the same as the minimum for every field.
    pub fn greatest_minimum(&self, field: usize) -> i32 {
        MINIMUM[field]
    }

    /// `getLeastMaximum(field)`. The week fields' least maxima are those of the first Gregorian day, whose month and
    /// year are the shortest; they depend on the week rules.
    pub fn least_maximum(&self, field: usize) -> i32 {
        match field {
            WEEK_OF_YEAR | WEEK_OF_MONTH => {
                let mut c =
                    GregorianCalendar::new(GREGORIAN_CUTOVER, &TimeZone::gmt(), &Locale::root());
                c.first_day = self.first_day;
                c.min_days = self.min_days;
                c.fields_from_time();
                c.actual_maximum(field).min(LEAST_MAXIMUM[field])
            }
            _ => LEAST_MAXIMUM[field],
        }
    }

    /// `getActualMinimum(field)`: with the default Gregorian change every month and year starts at the minimum.
    pub fn actual_minimum(&mut self, field: usize) -> i32 {
        MINIMUM[field]
    }

    /// `getActualMaximum(field)`: the largest value the field can take with the other fields as they are.
    pub fn actual_maximum(&self, field: usize) -> i32 {
        if !(self.time_valid && self.fields_valid) {
            let mut c = self.clone();
            return match c.complete() {
                Ok(()) => c.actual_maximum(field),
                Err(_) => MAXIMUM[field],
            };
        }
        let rules = self.rules();
        let fixed = self.current_day();
        let year = self.current_year();
        let month = i64::from(self.values[MONTH]);
        let dow = i64::from(self.values[DAY_OF_WEEK]);
        let first = month_start(year, month);
        let last = month_start(year, month + 1) - 1;
        // The last day of the month with the current day of week.
        let last_same = last - (i64::from(day_of_week(last)) - dow).rem_euclid(7);
        match field {
            YEAR => self.max_year(),
            DAY_OF_MONTH => civil_from_fixed(last).day,
            DAY_OF_YEAR => (year_start(year + 1) - year_start(year)) as i32,
            // Before the common era the JDK counts the weeks as if the year began on the weekday of January 1 of
            // the proleptic Julian year with the same number after it.
            WEEK_OF_YEAR if year <= 0 => {
                let start = year_start(year);
                let length = year_start(year + 1) - start;
                let first = rules.week_one_as(start, day_of_week(julian_fixed(1 - year, 1, 1)));
                let next_dow = day_of_week(julian_fixed(1 - year, 1, 1) + length);
                ((rules.week_one_as(start + length, next_dow) - first) / 7) as i32
            }
            WEEK_OF_YEAR => rules.weeks_in(year) as i32,
            // Only in the year of the Gregorian change does the last week depend on the day of week. Before the
            // common era the JDK lays the month out in the year with the same number after it.
            WEEK_OF_MONTH if year <= 0 => {
                let ce_first = julian_fixed(1 - year, month as i32 + 1, 1);
                let (next_year, next_month) = if month == 11 {
                    (2 - year, 1)
                } else {
                    (1 - year, month as i32 + 2)
                };
                let ce_last = julian_fixed(next_year, next_month, 1) - 1;
                rules.week_number(ce_last, ce_first) as i32
            }
            WEEK_OF_MONTH => rules.week_number(
                if year == CUTOVER_YEAR {
                    last_same
                } else {
                    last
                },
                first,
            ) as i32,
            DAY_OF_WEEK_IN_MONTH => ((last_same - first) / 7 + 1) as i32,
            _ => {
                let _ = fixed;
                MAXIMUM[field]
            }
        }
    }

    /// The largest `YEAR` at which the same date and time of day still fit in a `long` instant.
    fn max_year(&self) -> i32 {
        // The JDK compares the position in the year (day of the year and time of day, as milliseconds) with that of
        // the last (or, before the common era, the first) instant a `long` holds, in UTC. The calendar's position is
        // its local one moved back by its zone offset, twice before the common era (both observed).
        let position = |time: i64| {
            let fixed = time.div_euclid(DAY_MS) + EPOCH_OFFSET;
            (fixed - year_start(civil_from_fixed(fixed).year)) * DAY_MS + time.rem_euclid(DAY_MS)
        };
        let tod = ((i64::from(self.values[HOUR_OF_DAY]) * 60 + i64::from(self.values[MINUTE]))
            * 60
            + i64::from(self.values[SECOND]))
            * 1000
            + i64::from(self.values[MILLISECOND]);
        let offset = i64::from(self.values[ZONE_OFFSET]) + i64::from(self.values[DST_OFFSET]);
        let times = if self.values[ERA] == BCE { 2 } else { 1 };
        let here = (i64::from(self.values[DAY_OF_YEAR]) - 1) * DAY_MS + tod - times * offset;
        if self.values[ERA] == BCE {
            if here < position(i64::MIN) {
                MAX_YEAR_BCE - 1
            } else {
                MAX_YEAR_BCE
            }
        } else if here > position(i64::MAX) {
            MAX_YEAR_CE - 1
        } else {
            MAX_YEAR_CE
        }
    }

    /// `getWeekYear()`: the year the current week belongs to (1 BC is 0, earlier years are negative).
    pub fn week_year(&mut self) -> i32 {
        let _ = self.complete();
        self.week_year_and_weeks().0 as i32
    }

    /// `getWeeksInWeekYear()`.
    pub fn weeks_in_week_year(&mut self) -> Result<i32, JError> {
        self.complete()?;
        let (week_year, weeks) = self.week_year_and_weeks();
        // A non-lenient JDK calendar rejects its own week date before the common era.
        if !self.lenient && self.values[ERA] == BCE && week_year <= 0 {
            return Err(illegal_bare());
        }
        Ok(weeks as i32)
    }

    /// The week year and its number of weeks. Before the common era the JDK decides both on the same month and day
    /// of the Julian year with the same number after it (`YEAR` without the era), so the week year changes on other
    /// days than `WEEK_OF_YEAR` does.
    fn week_year_and_weeks(&self) -> (i64, i64) {
        let rules = self.rules();
        let year = self.current_year();
        if year > 0 {
            let week_year = rules.week_of_year(self.current_day(), year).1;
            return (week_year, rules.weeks_of_week_year(week_year));
        }
        // Before the common era the next week year starts where it would if the next January 1 fell on the
        // weekday of January 1 of the year numbered `YEAR - 1` after the common era (as observed in the JDK).
        let fixed = self.current_day();
        let start = year_start(year);
        let next = year_start(year + 1);
        let week_year = if fixed < rules.week_one(start) {
            year - 1
        } else if fixed >= rules.week_one_as(next, day_of_week(year_start(-year))) {
            year + 1
        } else {
            year
        };
        (week_year, rules.weeks_of_week_year(week_year))
    }

    /// `setWeekDate(weekYear, weekOfYear, dayOfWeek)`: resolved at once, keeping the time of day.
    pub fn set_week_date(
        &mut self,
        week_year: i32,
        week_of_year: i32,
        day_of_week: i32,
    ) -> Result<(), JError> {
        if !(SUNDAY..=SATURDAY).contains(&day_of_week) {
            return Err(illegal(format!("invalid dayOfWeek: {day_of_week}")));
        }
        // The week year counts in the era of the pending fields, resolved leniently (observed: a pending field that
        // moves the date across the start of the common era changes the era; an invalid `ERA` throws here).
        let era = {
            let mut resolved = self.clone();
            resolved.lenient = true;
            resolved.complete()?;
            resolved.values[ERA]
        };
        let year = if era == BCE {
            1 - i64::from(week_year)
        } else {
            i64::from(week_year)
        };
        let rules = self.rules();
        // A non-lenient JDK calendar also rejects its own week dates before the common era.
        let weeks = rules.weeks_of_week_year(year);
        if !self.lenient
            && (era == BCE && year <= 0 || !(1..=weeks).contains(&i64::from(week_of_year)))
        {
            return Err(illegal_bare());
        }
        let day = rules.week_one(year_start(year))
            + (i64::from(week_of_year) - 1) * 7
            + rules.index(i64::from(day_of_week));
        let date = civil_from_fixed(day);
        self.set_day(day);
        // The week of the year keeps its stamp but takes the new value; the day of week stays as it was (and is
        // checked against the result when non-lenient).
        self.values[WEEK_OF_YEAR] = rules.week_of_year(day, date.year).0 as i32;
        self.complete()
    }

    /// `equals(other)`: the same instant, leniency, week rules and zone.
    pub fn java_equals(&mut self, other: &mut GregorianCalendar) -> bool {
        let (Ok(a), Ok(b)) = (
            self.checked_time_in_millis(),
            other.checked_time_in_millis(),
        ) else {
            return false;
        };
        a == b
            && self.lenient == other.lenient
            && self.first_day == other.first_day
            && self.min_days == other.min_days
            && self.zone.id() == other.zone.id()
    }

    /// `hashCode()`: the instant folded with the leniency, week rules, zone and the Gregorian change day, laid out
    /// as observed in the JDK. The JDK's zone hash comes from its zone tables: it is 0 for `GMT` (observed), and
    /// for other zones this crate's `TimeZone.hashCode()` stands in.
    pub fn java_hash_code(&mut self) -> i32 {
        let t = self.time_in_millis();
        let zone_hash = if self.zone.id() == "GMT" {
            0
        } else {
            string_hash(self.zone.id()) ^ self.zone.raw_offset()
        };
        let others = i32::from(self.lenient)
            | (self.first_day << 1)
            | (self.min_days << 4)
            | (zone_hash << 7);
        (t as i32) ^ ((t >> 32) as i32) ^ others ^ (CUTOVER_DAY as i32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateCombination {
    /// `YEAR + MONTH + DAY_OF_MONTH`
    DayOfMonth,
    /// `YEAR + MONTH + WEEK_OF_MONTH + DAY_OF_WEEK`
    WeekOfMonth,
    /// `YEAR + MONTH + DAY_OF_WEEK_IN_MONTH + DAY_OF_WEEK`
    DayOfWeekInMonth,
    /// `YEAR + DAY_OF_YEAR`
    DayOfYear,
    /// `YEAR + DAY_OF_WEEK + WEEK_OF_YEAR`
    WeekOfYear,
}

impl fmt::Debug for GregorianCalendar {
    /// The shape of `Calendar.toString()`, with `?` for unset fields.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let time = if self.time_valid {
            self.time.to_string()
        } else {
            "?".into()
        };
        write!(
            f,
            "java.util.GregorianCalendar[time={time},areFieldsSet={},lenient={},zone={},firstDayOfWeek={},minimalDaysInFirstWeek={}",
            self.fields_valid,
            self.lenient,
            self.zone.id(),
            self.first_day,
            self.min_days
        )?;
        for (i, name) in FIELD_NAMES.iter().enumerate() {
            if self.stamps[i] == UNSET {
                write!(f, ",{name}=?")?;
            } else {
                write!(f, ",{name}={}", self.values[i])?;
            }
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers() {
        assert_eq!(gregorian_fixed(1970, 1, 1), EPOCH_OFFSET);
        assert_eq!(gregorian_fixed(1, 1, 1), 1);
        assert_eq!(julian_fixed(1582, 10, 5), CUTOVER_DAY);
        assert_eq!(julian_fixed(1, 1, 3), 1);
        for fd in [
            -800_000, -1, 0, 1, 577_735, 577_736, 719_163, 730_120, 3_000_000,
        ] {
            let g = gregorian_from_fixed(fd);
            assert_eq!(gregorian_fixed(g.year, g.month, i64::from(g.day)), fd);
            let j = julian_from_fixed(fd);
            assert_eq!(julian_fixed(j.year, j.month, i64::from(j.day)), fd);
        }
        assert_eq!(day_of_week(EPOCH_OFFSET), 5);
        let d = civil_from_fixed(CUTOVER_DAY - 1);
        assert_eq!((d.year, d.month, d.day, d.julian), (1582, 10, 4, true));
    }

    /// The state line of the JDK recordings (see the fixture header).
    fn state(c: &mut GregorianCalendar) -> String {
        let t = match c.checked_time_in_millis() {
            Ok(t) => t,
            Err(e) => return format!("E:{e}"),
        };
        let f: Vec<String> = (0..FIELD_COUNT).map(|i| c.get(i).to_string()).collect();
        let m: Vec<String> = [
            YEAR,
            MONTH,
            WEEK_OF_YEAR,
            WEEK_OF_MONTH,
            DAY_OF_MONTH,
            DAY_OF_YEAR,
            DAY_OF_WEEK_IN_MONTH,
        ]
        .iter()
        .map(|i| c.actual_maximum(*i).to_string())
        .collect();
        let week_year = c.week_year();
        match c.weeks_in_week_year() {
            Ok(w) => format!("{t};{};{};{week_year};{w}", f.join(","), m.join(",")),
            Err(e) => format!("E:{e}"),
        }
    }

    /// Replays one recorded case; `Err` carries what this calendar answered instead.
    fn replay(line: &str) -> Result<(), String> {
        let p: Vec<&str> = line.split('|').collect();
        let (language, country) = p[1].split_once('_').unwrap_or((p[1], ""));
        let locale = Locale::new(language, country, "").with_data(crate::text::LocaleData::Full);
        let mut c = GregorianCalendar::new(p[2].parse().unwrap(), &TimeZone::get(p[0]), &locale);
        let mut answer = None;
        for op in p[3].split(' ').filter(|s| !s.is_empty()) {
            let (op, expected) = op.split_once('=').map_or((op, None), |(a, b)| (a, Some(b)));
            let o: Vec<&str> = op.split(':').collect();
            let n = |i: usize| o[i].parse::<i32>().unwrap();
            let r = match o[0] {
                "add" => c.add(n(1) as usize, n(2)),
                "roll" => c.roll(n(1) as usize, n(2)),
                "set" => {
                    c.set(n(1) as usize, n(2));
                    Ok(())
                }
                "get" => c.checked_get(n(1) as usize).and_then(|v| match expected {
                    Some(e) if e != v.to_string() => Err(JError::new("get", format!("{op}={v}"))),
                    _ => Ok(()),
                }),
                "clear" => {
                    c.clear();
                    Ok(())
                }
                "clr" => {
                    c.clear_field(n(1) as usize);
                    Ok(())
                }
                "nl" => {
                    c.set_lenient(false);
                    Ok(())
                }
                "swd" => c.set_week_date(n(1), n(2), n(3)),
                "fd" => c.set_first_day_of_week(n(1)),
                "md" => c.set_minimal_days_in_first_week(n(1)),
                other => panic!("unknown op {other}"),
            };
            if let Err(e) = r {
                answer = Some(format!("E:{e}"));
                break;
            }
        }
        let answer = answer.unwrap_or_else(|| state(&mut c));
        if answer == p[4] { Ok(()) } else { Err(answer) }
    }

    /// The corners aligned with JDK 21: week-rule setters failing on an invalid pending
    /// era, `roll(WEEK_OF_YEAR)` from days of the neighbouring week year, `roll(DAY_OF_WEEK)` near the year's
    /// edges, `getActualMaximum(YEAR)` in other zones, `setWeekDate` with pending fields, and 1582.
    #[test]
    fn jdk_recorded_corners() {
        let text = include_str!("../../tests/fixtures/jdk21/calendar_corners.txt");
        let mut failures = Vec::new();
        let mut cases = 0;
        for line in text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            cases += 1;
            if let Err(got) = replay(line) {
                failures.push(format!("{line}\n   got {got}"));
            }
        }
        assert!(cases > 2000, "{cases} cases");
        assert!(
            failures.is_empty(),
            "{} of {cases} differ:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn week_rule_setters_keep_the_value_and_fail_on_an_invalid_pending_era() {
        let mut c = GregorianCalendar::new(0, &TimeZone::gmt(), &Locale::root());
        c.set(ERA, 2);
        let e = c.set_minimal_days_in_first_week(5).unwrap_err();
        assert_eq!(
            e.to_string(),
            "java.lang.IllegalArgumentException: Invalid era"
        );
        assert_eq!(c.minimal_days_in_first_week(), 5);
        // The same value again is no change, so nothing is resolved.
        assert!(c.set_minimal_days_in_first_week(5).is_ok());
        // With the instant valid nothing is pending.
        let mut c = GregorianCalendar::new(0, &TimeZone::gmt(), &Locale::root());
        assert!(c.set_first_day_of_week(5).is_ok());
    }
}
