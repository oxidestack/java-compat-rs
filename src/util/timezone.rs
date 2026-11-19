//! `java.util.TimeZone` as `TimeZone.getTimeZone(id)` returns it (`sun.util.calendar.ZoneInfo`), backed by the
//! IANA database in `chrono-tz`.

use std::sync::Arc;

use chrono::{DateTime, Offset, TimeZone as _};
use chrono_tz::{OffsetComponents, Tz};

use crate::text::locale::{Lang, Locale};

use super::tz_names::ZONE_NAMES;

/// `TimeZone.SHORT` / `TimeZone.LONG`.
pub const SHORT: i32 = 0;
/// `TimeZone.LONG`.
pub const LONG: i32 = 1;

#[derive(Clone, Debug)]
enum Rules {
    /// A fixed offset (GMT, custom `GMT+hh:mm` IDs).
    Fixed(i32),
    Tz(Tz),
}

/// A time zone: its ID and offset rules.
#[derive(Clone, Debug)]
pub struct TimeZone {
    id: Arc<str>,
    rules: Rules,
}

impl PartialEq for TimeZone {
    fn eq(&self, other: &TimeZone) -> bool {
        self.id == other.id
    }
}

/// The JDK's deprecated three-letter IDs that are not IANA zones themselves.
const OLD_IDS: &[(&str, &str)] = &[
    ("ACT", "Australia/Darwin"),
    ("AET", "Australia/Sydney"),
    ("AGT", "America/Argentina/Buenos_Aires"),
    ("ART", "Africa/Cairo"),
    ("AST", "America/Anchorage"),
    ("BET", "America/Sao_Paulo"),
    ("BST", "Asia/Dhaka"),
    ("CAT", "Africa/Harare"),
    ("CNT", "America/St_Johns"),
    ("CST", "America/Chicago"),
    ("CTT", "Asia/Shanghai"),
    ("EAT", "Africa/Addis_Ababa"),
    ("ECT", "Europe/Paris"),
    ("IET", "America/Indiana/Indianapolis"),
    ("IST", "Asia/Kolkata"),
    ("JST", "Asia/Tokyo"),
    ("MIT", "Pacific/Apia"),
    ("NET", "Asia/Yerevan"),
    ("NST", "Pacific/Auckland"),
    ("PLT", "Asia/Karachi"),
    ("PNT", "America/Phoenix"),
    ("PRT", "America/Puerto_Rico"),
    ("PST", "America/Los_Angeles"),
    ("SST", "Pacific/Guadalcanal"),
    ("VST", "Asia/Ho_Chi_Minh"),
];

const MS_PER_HOUR: i64 = 3_600_000;
/// 2100-01-01T00:00Z, where chrono-tz's tables end.
const UTC2100: i64 = 4_102_444_800_000;
/// 1900-01-01T00:00Z, where the JDK's zone tables begin.
const UTC1900: i64 = -2_208_988_800_000;
/// 2040-01-15T00:00Z, an instant governed by each zone's last rule.
const LAST_RULE_PROBE: i64 = 2_210_112_000_000;

impl TimeZone {
    /// `TimeZone.getTimeZone("GMT")`.
    pub fn gmt() -> TimeZone {
        TimeZone { id: "GMT".into(), rules: Rules::Fixed(0) }
    }

    /// `TimeZone.getDefault()` as the ports assume it: GMT.
    pub fn default_zone() -> TimeZone {
        TimeZone::gmt()
    }

    /// `TimeZone.getTimeZone(id)`: an IANA ID (case-sensitive), a JDK three-letter ID, a custom `GMT±h[h][[:]mm]`
    /// ID (normalised to `GMT±hh:mm`), or GMT when the ID is not understood.
    pub fn get(id: &str) -> TimeZone {
        TimeZone::find(id).unwrap_or_else(TimeZone::gmt)
    }

    /// `TimeZone.getTimeZone(id)` without the GMT fallback: `None` for an unknown ID.
    pub fn find(id: &str) -> Option<TimeZone> {
        if id == "GMT" {
            return Some(TimeZone::gmt());
        }
        if let Some((_, target)) = OLD_IDS.iter().find(|(old, _)| *old == id) {
            let tz: Tz = target.parse().ok()?;
            return Some(TimeZone { id: id.into(), rules: Rules::Tz(tz) });
        }
        if let Ok(tz) = id.parse::<Tz>() {
            return Some(TimeZone { id: id.into(), rules: Rules::Tz(tz) });
        }
        parse_custom(id)
    }

    /// A fixed-offset zone with a custom ID (`GMT+01:00`) for an offset in milliseconds.
    pub fn fixed(offset_millis: i32) -> TimeZone {
        TimeZone { id: custom_id(offset_millis).into(), rules: Rules::Fixed(offset_millis) }
    }

    /// `getID()`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// `(raw offset, DST saving)` in effect at an instant, as `ZoneInfo.getOffsets(utc)` returns them.
    pub fn offsets_at(&self, millis: i64) -> (i32, i32) {
        match &self.rules {
            Rules::Fixed(o) => (*o, 0),
            Rules::Tz(tz) => {
                // The JDK's ZoneInfo tables start in 1900; earlier instants get the last standard offset.
                if millis < UTC1900 {
                    return (self.tz_offsets(tz, LAST_RULE_PROBE).0, 0);
                }
                self.tz_offsets(tz, millis)
            }
        }
    }

    fn tz_offsets(&self, tz: &Tz, millis: i64) -> (i32, i32) {
        // chrono-tz's tables end in 2100; the JDK keeps applying the last rule. Shift later instants to an
        // equivalent year (same weekday of January 1, same leap-ness) before 2100.
        let millis = if millis >= UTC2100 { shift_before_2100(millis) } else { millis };
        let clamped = millis.clamp(-8_000_000_000_000_000, 8_000_000_000_000_000);
        let Some(dt) = DateTime::from_timestamp_millis(clamped) else { return (0, 0) };
        let off = tz.offset_from_utc_datetime(&dt.naive_utc());
        let total = off.fix().local_minus_utc() as i64 * 1000;
        let dst = off.dst_offset().num_milliseconds();
        ((total - dst) as i32, dst as i32)
    }

    /// `getOffset(date)`: the total offset from UTC at an instant.
    pub fn offset(&self, millis: i64) -> i32 {
        let (raw, dst) = self.offsets_at(millis);
        raw + dst
    }

    /// `ZoneInfo.getOffsetsByWall(wall)`: the offsets for a local wall-clock time (milliseconds since the local
    /// epoch). In a gap the offset before the transition applies; in an overlap, the one after it.
    pub fn offsets_by_wall(&self, wall: i64) -> (i32, i32) {
        if let Rules::Fixed(o) = self.rules {
            return (o, 0);
        }
        let day = 86_400_000;
        let mut candidates: Vec<i64> = [wall - day, wall, wall + day].iter().map(|t| self.offset(*t) as i64).collect();
        candidates.sort();
        candidates.dedup();
        // A fixed point: offset(wall - o) == o. Prefer the later instant (the smaller offset).
        for o in &candidates {
            if self.offset(wall - o) as i64 == *o {
                return self.offsets_at(wall - o);
            }
        }
        let largest = *candidates.last().expect("candidates");
        self.offsets_at(wall - largest)
    }

    /// `inDaylightTime(date)`.
    pub fn in_daylight_time(&self, millis: i64) -> bool {
        self.offsets_at(millis).1 != 0
    }

    /// `getRawOffset()`: the current standard offset.
    pub fn raw_offset(&self) -> i32 {
        self.offsets_at(now_millis()).0
    }

    /// `useDaylightTime()`: whether the zone's current rules have daylight saving time.
    pub fn use_daylight_time(&self) -> bool {
        self.dst_savings() != 0
    }

    /// `getDSTSavings()`.
    pub fn dst_savings(&self) -> i32 {
        match &self.rules {
            Rules::Fixed(_) => 0,
            Rules::Tz(_) => {
                // Probe a year well after the current one, as the JDK looks at the last rule.
                let year = 2040i64;
                let start = (year - 1970) * 365 * 86_400_000 + 10 * 86_400_000;
                (0..12).map(|m| self.offsets_at(start + m * 30 * 86_400_000).1).max().unwrap_or(0)
            }
        }
    }

    /// `getDisplayName(daylight, style, locale)`.
    pub fn display_name(&self, daylight: bool, style: i32, locale: &Locale) -> String {
        let index = match (daylight, style == SHORT) {
            (false, false) => 0,
            (false, true) => 1,
            (true, false) => 2,
            (true, true) => 3,
        };
        if let Some(name) = recorded_name(&self.id, index, locale) {
            return name;
        }
        let offset = match &self.rules {
            Rules::Fixed(_) => return self.id.to_string(),
            Rules::Tz(_) => self.raw_offset() + if daylight { self.dst_savings().max(MS_PER_HOUR as i32) } else { 0 },
        };
        // Zones the tables lack. `Etc/GMT±N` has no names in the JDK either: each locale writes the offset in its
        // GMT format, the same for all four variants, as it does for the recorded Etc/GMT+5 and Etc/GMT-3.
        if let Some(hours) = etc_gmt_hours(&self.id) {
            let (template, template_hours) = if hours < 0 { ("Etc/GMT+5", 5) } else { ("Etc/GMT-3", 3) };
            return recorded_name(template, index, locale)
                .and_then(|name| replace_hours(&name, template_hours, hours.unsigned_abs()))
                .unwrap_or_else(|| custom_id(self.raw_offset()));
        }
        // Etc/GMT and its links: GMT's long names; the short ones are the locale's zero GMT format, the prefix of
        // its offset format (Etc/GMT+5's short name before the sign).
        if ETC_GMT_ZERO.contains(&&*self.id) && index % 2 == 1 {
            if let Some(prefix) = recorded_name("Etc/GMT+5", index, locale).and_then(|name| gmt_prefix(&name)) {
                return prefix;
            }
        }
        // The UTC links share UTC's names; the other zones at GMT all year carry the GMT names (Africa/Monrovia: "GMT").
        if self.raw_offset() == 0 && !self.use_daylight_time() {
            return recorded_name(if UTC_LINKS.contains(&&*self.id) { "UTC" } else { "GMT" }, index, locale)
                .unwrap_or_else(|| custom_id(0));
        }
        custom_id(offset)
    }

    /// `getDisplayName(locale)`: the standard long name.
    pub fn display_name_long(&self, locale: &Locale) -> String {
        self.display_name(false, LONG, locale)
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The names this crate recorded for a zone (`index`: std long, std short, dst long, dst short): the locale's
/// recorded date data first, then the root, English and French table.
fn recorded_name(id: &str, index: usize, locale: &Locale) -> Option<String> {
    if let Some((.., row)) = crate::text::symbols::recorded_dates(locale) {
        use crate::text::locale_dates::{ZONE_IDS, ZONE_STRINGS};
        if let Some(z) = ZONE_IDS.iter().position(|z| *z == id) {
            // Recorded order: std long, std short, dst long, dst short.
            return Some(ZONE_STRINGS[row[z * 4 + index] as usize].to_string());
        }
    }
    let (_, root, en, fr) = ZONE_NAMES.iter().find(|(z, ..)| *z == id)?;
    let names = match locale.lang() {
        Lang::En => en,
        Lang::Fr => fr,
        Lang::Root => root,
    };
    Some(names[index].to_string())
}

/// `name` with its two-digit hours `from` (the first two adjacent digits zero and `from`, in any Unicode decimal
/// digit block) written as `to` in the same digits; `None` when `name` has no such digits.
fn replace_hours(name: &str, from: u32, to: u32) -> Option<String> {
    let chars: Vec<char> = name.chars().collect();
    let at = chars.windows(2).position(|w| w[0].is_numeric() && w[1] as u32 == w[0] as u32 + from)?;
    let zero = chars[at] as u32;
    let digit = |d: u32| char::from_u32(zero + d);
    let mut out: String = chars[..at].iter().collect();
    out.push(digit(to / 10)?);
    out.push(digit(to % 10)?);
    out.extend(&chars[at + 2..]);
    Some(out)
}

/// The text before the sign of an offset written as Etc/GMT+5's name (`UTC−05:00` → `UTC`).
fn gmt_prefix(name: &str) -> Option<String> {
    let chars: Vec<char> = name.chars().collect();
    let at = chars.windows(2).position(|w| w[0].is_numeric() && w[1] as u32 == w[0] as u32 + 5)?;
    let sign = at.checked_sub(1)?;
    let prefix: String = chars[..sign].iter().collect();
    let prefix = prefix.trim_end();
    (!prefix.is_empty()).then(|| prefix.to_string())
}

/// `Etc/GMT` and its links, the zero-offset zones the JDK names by the GMT format rather than by a metazone.
const ETC_GMT_ZERO: &[&str] = &["Etc/GMT", "Etc/GMT0", "Etc/GMT+0", "Etc/GMT-0", "GMT0"];

/// The IANA links to UTC other than `UTC` and `Etc/UTC` themselves.
const UTC_LINKS: &[&str] = &["Etc/UCT", "Etc/Universal", "Etc/Zulu", "UCT", "Universal", "Zulu"];

/// The offset in hours of a non-zero `Etc/GMT±N` zone (POSIX sign: `Etc/GMT+5` is five hours behind GMT).
fn etc_gmt_hours(id: &str) -> Option<i32> {
    let rest = id.strip_prefix("Etc/GMT")?;
    let (sign, digits) = match rest.as_bytes().first()? {
        b'+' => (-1, &rest[1..]),
        b'-' => (1, &rest[1..]),
        _ => return None,
    };
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits.parse().ok()?;
    (hours != 0).then_some(sign * hours)
}

/// `ZoneInfoFile.toCustomID(offset)`: `GMT+01:00`, `GMT-05:30`.
pub(crate) fn custom_id(offset_millis: i32) -> String {
    let (sign, secs) = if offset_millis >= 0 { ('+', offset_millis / 1000) } else { ('-', -(offset_millis / 1000)) };
    let (hh, mm, ss) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let mut id = format!("GMT{sign}{hh:02}:{mm:02}");
    if ss != 0 {
        id.push_str(&format!(":{ss:02}"));
    }
    id
}

/// `TimeZone.parseCustomTimeZone`: `GMT+5`, `GMT-05:30`, `GMT+0130`.
fn parse_custom(id: &str) -> Option<TimeZone> {
    let rest = id.strip_prefix("GMT")?;
    if rest.len() < 2 {
        return None;
    }
    let (negative, body) = match rest.as_bytes()[0] {
        b'-' => (true, &rest[1..]),
        b'+' => (false, &rest[1..]),
        _ => return None,
    };
    let (mut hours, mut minutes, mut num, mut delims, mut len) = (0i32, 0i32, 0i32, 0, 0);
    for c in body.chars() {
        if c == ':' {
            if delims > 1 || len == 0 || len > 2 {
                return None;
            }
            if delims == 0 {
                hours = num;
            } else {
                minutes = num;
            }
            delims += 1;
            num = 0;
            len = 0;
            continue;
        }
        let d = c.to_digit(10)? as i32;
        num = num * 10 + d;
        len += 1;
    }
    match delims {
        0 => {
            if len <= 2 {
                hours = num;
                minutes = 0;
            } else if len <= 4 {
                hours = num / 100;
                minutes = num % 100;
            } else {
                return None;
            }
            num = 0;
        }
        1 => {
            if len != 2 {
                return None;
            }
            minutes = num;
            num = 0;
        }
        _ => {
            if len != 2 {
                return None;
            }
        }
    }
    if hours > 23 || minutes > 59 || num > 59 {
        return None;
    }
    let offset = (hours * 3600 + minutes * 60 + num) * 1000;
    if offset == 0 {
        return Some(TimeZone { id: if negative { "GMT-00:00" } else { "GMT+00:00" }.into(), rules: Rules::Fixed(0) });
    }
    Some(TimeZone::fixed(if negative { -offset } else { offset }))
}

/// An instant in an equivalent year of 2072–2099 (same January 1 weekday and leap-ness), same wall position.
fn shift_before_2100(millis: i64) -> i64 {
    use super::calendar::{EPOCH_OFFSET, day_of_week, gregorian_fixed, gregorian_from_fixed};
    let day = 86_400_000i64;
    let fixed = millis.div_euclid(day) + EPOCH_OFFSET;
    let year = gregorian_from_fixed(fixed).year;
    let leap = |y: i64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let jan1 = gregorian_fixed(year, 1, 1);
    let target = (2072..2100).find(|y| leap(*y) == leap(year) && day_of_week(gregorian_fixed(*y, 1, 1)) == day_of_week(jan1)).unwrap_or(2072);
    millis - (jan1 - gregorian_fixed(target, 1, 1)) * day
}
