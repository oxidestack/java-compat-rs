import java.nio.file.Path;
import java.text.DateFormat;
import java.text.DateFormatSymbols;
import java.text.ParsePosition;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Calendar;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.TimeZone;
import java.util.TreeSet;

/**
 * Date locale data of every available locale of the languages LocaleNumberFixtures carries (Keycloak 26.5.3's theme
 * languages, English, French), built from language and country only: DateFormatSymbols (eras, months, weekdays,
 * AM/PM), the standalone month names SimpleDateFormat's 'L' takes from Calendar.getDisplayName, the DateFormat style
 * patterns, the display names of the zones src/util/tz_names.rs carries, and the week data Calendar takes from a
 * locale's region. Each locale also carries what SimpleDateFormat and the DateFormat instances render and parse with
 * that data. Written to locale_dates.json, week_data.json and jdk_languages.json; src/text/locale_dates.rs is generated from it
 * (reference/gen_locale_dates.py).
 */
final class LocaleDateFixtures {
    static final String[] ZONES = {"GMT", "UTC", "Europe/Paris", "America/New_York", "Europe/London", "America/Los_Angeles",
        "America/Chicago", "Asia/Tokyo", "Australia/Sydney", "Europe/Berlin", "America/Mexico_City", "Asia/Kolkata", "America/Denver",
        "Europe/Moscow", "America/Sao_Paulo", "Asia/Shanghai", "Pacific/Auckland", "America/Phoenix", "Pacific/Honolulu", "Africa/Cairo",
        "America/Toronto", "Europe/Madrid", "Europe/Rome", "Europe/Amsterdam", "Europe/Brussels", "Europe/Zurich", "Asia/Dubai",
        "Asia/Singapore", "Asia/Hong_Kong", "Etc/GMT+5", "Etc/GMT-3", "Etc/UTC", "EST", "PST", "CET", "MST", "HST", "EET", "WET", "MET",
        "CST", "IST", "JST", "ECT", "AST", "BST"};

    /** Every text field, and numeric ones whose digits or week data are the locale's. */
    static final String FIELDS = "G GGGG yyyy|M MMM MMMM MMMMM|L LLL LLLL LLLLL|EEE EEEE|d hh HH a|D w W F Y u|z zzzz Z";

    static final String[] PARSE_PATTERNS = {"d MMMM yyyy", "d MMM yyyy", "LLLL yyyy", "LLL yyyy", "EEEE, d MMMM yyyy G", "h:mm a",
        "EEE HH:mm", "yyyy-MM-dd"};

    /** 2026-01-15 13:05:09.123, 2026-02-15 06:05:09.123, 2026-07-15 06:05:09.123, 2026-12-15 13:05:09.123 GMT, and
     * 0001-12-31 BC. */
    static final long[] INSTANTS = {1768482309123L, 1771135509123L, 1784095509123L, 1797339909123L, -62198755200000L};

    static List<String> list(String[] a, int from) {
        return Arrays.asList(a).subList(from, a.length);
    }

    static void generate(Path dir) throws Exception {
        Set<String> tags = new TreeSet<>();
        for (Locale l : Locale.getAvailableLocales()) {
            if (LocaleNumberFixtures.LANGUAGES.contains(l.getLanguage())) {
                tags.add(l.getLanguage());
                if (!l.getCountry().isEmpty()) tags.add(l.getLanguage() + "_" + l.getCountry());
            }
        }
        // A language the JDK has no data for: root's symbols and patterns, but the fallback adapter's (English) zone
        // names.
        tags.add("xx");
        // The JDK's zone names depend on what the JVM asked before: CLDRTimeZoneNameProviderImpl fills a missing name
        // in the resource bundle's own array, and a child locale copies a parent's name once it exists. root's names
        // asked first (as TimeZoneFixtures does, and as the crate's root table is) make e.g. fr's short name of
        // Etc/GMT+5 root's "GMT-05:00" instead of fr's own "UTC\u221205:00".
        for (String id : ZONES) {
            TimeZone z = TimeZone.getTimeZone(id);
            for (boolean daylight : new boolean[]{false, true}) {
                z.getDisplayName(daylight, TimeZone.LONG, Locale.ROOT);
                z.getDisplayName(daylight, TimeZone.SHORT, Locale.ROOT);
            }
        }
        List<Object> locales = new ArrayList<>();
        for (String t : tags) {
            String[] p = t.split("_");
            Locale l = p.length == 1 ? new Locale(p[0]) : new Locale(p[0], p[1]);
            DateFormatSymbols s = DateFormatSymbols.getInstance(l);
            Calendar cal = Calendar.getInstance(l);
            List<String> standalone = new ArrayList<>(), standaloneShort = new ArrayList<>();
            for (int m = 0; m < 12; m++) {
                cal.set(Calendar.MONTH, m);
                cal.set(Calendar.DAY_OF_MONTH, 1);
                standalone.add(cal.getDisplayName(Calendar.MONTH, Calendar.LONG_STANDALONE, l));
                standaloneShort.add(cal.getDisplayName(Calendar.MONTH, Calendar.SHORT_STANDALONE, l));
            }
            List<Object> date = new ArrayList<>(), time = new ArrayList<>(), dateTime = new ArrayList<>();
            for (int a = 0; a < 4; a++) {
                date.add(((SimpleDateFormat) DateFormat.getDateInstance(a, l)).toPattern());
                time.add(((SimpleDateFormat) DateFormat.getTimeInstance(a, l)).toPattern());
                for (int b = 0; b < 4; b++) dateTime.add(((SimpleDateFormat) DateFormat.getDateTimeInstance(a, b, l)).toPattern());
            }
            List<Object> zones = new ArrayList<>();
            for (String id : ZONES) {
                TimeZone z = TimeZone.getTimeZone(id);
                zones.add(Json.list(id, z.getDisplayName(false, TimeZone.LONG, l), z.getDisplayName(false, TimeZone.SHORT, l),
                        z.getDisplayName(true, TimeZone.LONG, l), z.getDisplayName(true, TimeZone.SHORT, l)));
            }
            // What the data renders: the fields at each instant (GMT, and Paris for zone names in winter and summer),
            // every style instance at two instants.
            List<Object> formats = new ArrayList<>();
            SimpleDateFormat fields = new SimpleDateFormat(FIELDS, l);
            for (String zone : new String[]{"GMT", "Europe/Paris"}) {
                fields.setTimeZone(TimeZone.getTimeZone(zone));
                for (int i = 0; i < INSTANTS.length; i++) {
                    if (zone.equals("GMT") || i == 0 || i == 2) formats.add(Json.list(zone, Long.toString(INSTANTS[i]), fields.format(new Date(INSTANTS[i]))));
                }
            }
            List<Object> styles = new ArrayList<>();
            for (int a = -1; a < 4; a++) {
                for (int b = -1; b < 4; b++) {
                    if (a == -1 && b == -1) continue;
                    DateFormat f = a == -1 ? DateFormat.getTimeInstance(b, l) : b == -1 ? DateFormat.getDateInstance(a, l)
                        : DateFormat.getDateTimeInstance(a, b, l);
                    f.setTimeZone(TimeZone.getTimeZone("Europe/Paris"));
                    String summer = f.format(new Date(INSTANTS[2]));
                    f.setTimeZone(TimeZone.getTimeZone("GMT"));
                    styles.add(Json.list(a, b, summer, f.format(new Date(INSTANTS[1]))));
                }
            }
            // Formatted texts parsed back (lenient); for the first two patterns also upper-cased (names match ignoring
            // case).
            List<Object> parses = new ArrayList<>();
            for (int k = 0; k < PARSE_PATTERNS.length; k++) {
                String pattern = PARSE_PATTERNS[k];
                SimpleDateFormat f = new SimpleDateFormat(pattern, l);
                f.setTimeZone(TimeZone.getTimeZone("GMT"));
                f.set2DigitYearStart(new Date(-1577923200000L));
                for (long instant : INSTANTS) {
                    String text = f.format(new Date(instant));
                    List<String> inputs = k < 2 ? List.of(text, text.toUpperCase(Locale.ROOT)) : List.of(text);
                    for (String in : inputs) {
                        ParsePosition pos = new ParsePosition(0);
                        Date d = f.parse(in, pos);
                        parses.add(Json.list(pattern, in, d == null ? null : Long.toString(d.getTime()), pos.getIndex(), pos.getErrorIndex()));
                    }
                }
            }
            locales.add(Json.obj("locale", l.toString(), "calendar", cal.getClass().getSimpleName(), "eras", list(s.getEras(), 0),
                    "months", list(s.getMonths(), 0).subList(0, 12), "shortMonths", list(s.getShortMonths(), 0).subList(0, 12),
                    "standaloneMonths", standalone, "standaloneShortMonths", standaloneShort, "weekdays", list(s.getWeekdays(), 1),
                    "shortWeekdays", list(s.getShortWeekdays(), 1), "ampm", list(s.getAmPmStrings(), 0), "date", date, "time", time,
                    "dateTime", dateTime, "zones", zones, "fields", FIELDS, "formats", formats, "styles", styles, "parses", parses));
        }
        // Week data comes from the region alone (an empty one is the JDK's default, an unknown one CLDR's 001).
        Set<String> regions = new TreeSet<>(Arrays.asList(Locale.getISOCountries()));
        for (Locale l : Locale.getAvailableLocales()) regions.add(l.getCountry());
        regions.add("XX");
        List<Object> weeks = new ArrayList<>();
        for (String r : regions) {
            Calendar c = Calendar.getInstance(new Locale("", r));
            weeks.add(Json.list(r, c.getFirstDayOfWeek(), c.getMinimalDaysInFirstWeek()));
        }
        Json.writeUtf8(dir.resolve("locale_dates.json"), locales);
        Json.write(dir.resolve("week_data.json"), weeks);
        // The languages the JDK has locale data for: any other formats with root data, as the crate does.
        Set<String> languages = new TreeSet<>();
        for (Locale l : Locale.getAvailableLocales()) languages.add(l.getLanguage());
        Json.write(dir.resolve("jdk_languages.json"), new ArrayList<>(languages));
    }
}
