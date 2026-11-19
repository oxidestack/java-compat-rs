import java.nio.file.Path;
import java.text.DateFormat;
import java.text.DateFormatSymbols;
import java.text.ParsePosition;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import java.util.TimeZone;

/** DateFormatSymbols, DateFormat instances, SimpleDateFormat format and parse, Date.toString. */
final class DateFixtures {
    static final Locale[] LOCALES = {Locale.ROOT, Locale.ENGLISH, Locale.US, Locale.FRENCH, Locale.FRANCE};
    static final String[] ZONES = {"GMT", "Europe/Paris", "America/New_York", "Asia/Kolkata"};
    static final long[] INSTANTS = {0L, 1790322531000L, 1790322531007L, 1774746000000L, 1792890000000L, 1773558000000L, 1793512800000L,
        -1000000000000L, -12219292800001L, -62135596800001L, -100000000000000L, 253402300799999L, 1735603200000L, 1767139200000L,
        946684799999L, 951782400000L, 43200000L, 1790294400000L, 1790337600000L, -62198755200000L, 1000000000000000L};

    static final String[] PATTERNS = {
        "G GG GGGG GGGGG", "y yy yyy yyyy yyyyy", "Y YY YYY YYYY", "M MM MMM MMMM MMMMM", "L LL LLL LLLL LLLLL", "w ww W WW", "D DD DDD", "d dd",
        "F FF", "E EE EEE EEEE EEEEE", "u uu", "a aa", "H HH", "k kk", "K KK", "h hh", "m mm", "s ss", "S SS SSS SSSS", "z zz zzz zzzz",
        "Z ZZ ZZZZ", "X XX XXX", "''yyyy'' 'o''clock' ''", "EEE, d MMM yyyy HH:mm:ss Z", "yyyy-MM-dd'T'HH:mm:ss.SSSXXX", "MMMM", "LLLL",
        "EEEE d MMMM yyyy", "hh 'o''clock' a, zzzz", "K:mm a, z", "yyyyy.MMMMM.dd GGG hh:mm aaa", "yyMMddHHmmssZ", "dd/MM/yy", "'week' w 'of' Y",
        "é MMM", "yyyy MM", "M/d/yy h:mm a", "yyyyMMdd", "HHmmss", "yyyy-DDD", "u EEEE"
    };

    static final String[] BAD_PATTERNS = {"yyyy-MM-dd q", "'unterminated", "XXXX", "yyyy-MM-dd'", "abc", "e", "yyyy-MM-ddTHH", "B", "hh:mm:ss.SSS O"};

    static void generate(Path dir) throws Exception {
        List<Object> symbols = new ArrayList<>();
        for (Locale l : LOCALES) {
            DateFormatSymbols s = DateFormatSymbols.getInstance(l);
            symbols.add(Json.obj("locale", l.toString(), "eras", Arrays.asList(s.getEras()), "months", Arrays.asList(s.getMonths()),
                    "shortMonths", Arrays.asList(s.getShortMonths()), "weekdays", Arrays.asList(s.getWeekdays()),
                    "shortWeekdays", Arrays.asList(s.getShortWeekdays()), "ampm", Arrays.asList(s.getAmPmStrings()),
                    "localPatternChars", s.getLocalPatternChars()));
        }
        Json.write(dir.resolve("date_symbols.json"), symbols);

        List<Object> instances = new ArrayList<>();
        for (Locale l : LOCALES) {
            for (int ds = -1; ds < 4; ds++) {
                for (int ts = -1; ts < 4; ts++) {
                    if (ds == -1 && ts == -1) continue;
                    DateFormat f = ds == -1 ? DateFormat.getTimeInstance(ts, l) : ts == -1 ? DateFormat.getDateInstance(ds, l) : DateFormat.getDateTimeInstance(ds, ts, l);
                    List<Object> formats = new ArrayList<>();
                    for (String z : new String[]{"GMT", "America/New_York"}) {
                        f.setTimeZone(TimeZone.getTimeZone(z));
                        for (long t : new long[]{0L, 1790322531000L, 1774746000000L, -62135596800001L}) formats.add(Json.list(z, Long.toString(t), f.format(new Date(t))));
                    }
                    instances.add(Json.obj("locale", l.toString(), "dateStyle", ds, "timeStyle", ts, "pattern", ((SimpleDateFormat) f).toPattern(), "formats", formats));
                }
            }
        }
        Json.write(dir.resolve("date_instances.json"), instances);

        List<Object> patterns = new ArrayList<>();
        for (Locale l : new Locale[]{Locale.US, Locale.FRANCE, Locale.ROOT}) {
            for (String p : PATTERNS) {
                SimpleDateFormat f = new SimpleDateFormat(p, l);
                List<Object> formats = new ArrayList<>();
                for (String z : ZONES) {
                    f.setTimeZone(TimeZone.getTimeZone(z));
                    for (long t : INSTANTS) formats.add(Json.list(z, Long.toString(t), f.format(new Date(t))));
                }
                String localized;
                try {
                    localized = f.toLocalizedPattern();
                } catch (Exception e) {
                    localized = "ERROR " + Json.error(e);
                }
                patterns.add(Json.obj("locale", l.toString(), "pattern", p, "toPattern", f.toPattern(), "localized", localized, "formats", formats));
            }
        }
        for (String p : BAD_PATTERNS) {
            try {
                new SimpleDateFormat(p, Locale.US);
                patterns.add(Json.obj("locale", "en_US", "pattern", p, "error", null));
            } catch (Exception e) {
                patterns.add(Json.obj("locale", "en_US", "pattern", p, "error", Json.error(e)));
            }
        }
        Json.write(dir.resolve("date_patterns.json"), patterns);

        List<Object> dates = new ArrayList<>();
        for (String z : ZONES) {
            TimeZone.setDefault(TimeZone.getTimeZone(z));
            for (long t : INSTANTS) {
                Date d = new Date(t);
                @SuppressWarnings("deprecation")
                Object gmt = d.toGMTString();
                dates.add(Json.obj("zone", z, "time", Long.toString(t), "toString", d.toString(), "toGMTString", gmt, "hashCode", d.hashCode()));
            }
        }
        TimeZone.setDefault(TimeZone.getTimeZone("GMT"));
        Json.write(dir.resolve("date_tostring.json"), dates);

        generateParse(dir);
    }

    static Object parseCase(SimpleDateFormat f, String text, int index) {
        ParsePosition pos = new ParsePosition(index);
        Date d = f.parse(text, pos);
        return Json.obj("text", text, "index", index, "result", d == null ? null : Long.toString(d.getTime()), "pos", pos.getIndex(),
                "err", pos.getErrorIndex(), "zoneAfter", f.getTimeZone().getID());
    }

    static void generateParse(Path dir) throws Exception {
        Object[][] specs = {
            {"yyyy-MM-dd", new String[]{"2026-09-25", "2026-9-5", "2026-13-45", "2026-00-00", "26-09-25", "2026-09-25x", "2026/09/25", "  2026-09-25", "2026- 09-25",
                "-2026-09-25", "+2026-09-25", "2026-09", "", "99999-01-01", "2026-09-25 12:00", "0-1-1", "2026-02-29", "2024-02-29", "1582-10-10"}},
            {"yyyyMMdd", new String[]{"20260925", "2026925", "202609251", "2026092", "20260925T"}},
            {"yy-MM-dd", new String[]{"26-09-25", "99-12-31", "00-01-01", "2026-09-25", "5-09-25", "-5-09-25", "105-01-01"}},
            {"MMM d, yyyy", new String[]{"Sep 25, 2026", "sep 25, 2026", "SEPTEMBER 25, 2026", "September25, 2026", "Sept 25, 2026", "sept. 25, 2026",
                "janv. 5, 2026", "Janvier 5, 2026", "May 1, 2026", "Mai 1, 2026", "juin 1, 2026", "Jun 1, 2026", "25 Sep, 2026"}},
            {"EEE, d MMM yyyy HH:mm:ss Z", new String[]{"Fri, 25 Sep 2026 07:48:51 +0000", "Fri, 25 Sep 2026 07:48:51 +0200", "fri, 25 sep 2026 07:48:51 -0530",
                "Friday, 25 September 2026 07:48:51 GMT", "Fri, 25 Sep 2026 07:48:51 GMT+02:00", "Mon, 25 Sep 2026 07:48:51 +0000", "Fri, 25 Sep 2026 7:48:51 +0000",
                "Fri, 25 Sep 2026 07:48:51 +02", "Fri, 25 Sep 2026 07:48:51 UTC", "Fri, 25 Sep 2026 07:48:51 CEST", "Fri, 25 Sep 2026 07:48:51 PST",
                "Fri, 25 Sep 2026 07:48:51 Central European Summer Time", "ven., 25 sept. 2026 07:48:51 +0000", "Fri, 25 Sep 2026 24:61:61 +0000"}},
            {"yyyy-MM-dd'T'HH:mm:ss.SSSXXX", new String[]{"2026-09-25T07:48:51.123Z", "2026-09-25T07:48:51.123+02:00", "2026-09-25T07:48:51.123+0200",
                "2026-09-25T07:48:51.123+02", "2026-09-25T07:48:51.1Z", "2026-09-25T07:48:51.123456Z", "2026-09-25T07:48:51Z", "2026-09-25T07:48:51.123-00:00"}},
            {"yyyy-MM-dd'T'HH:mm:ssX", new String[]{"2026-09-25T07:48:51Z", "2026-09-25T07:48:51+02", "2026-09-25T07:48:51+0230", "2026-09-25T07:48:51+2"}},
            {"h:mm a", new String[]{"7:48 AM", "7:48 PM", "12:00 AM", "12:00 PM", "0:30 PM", "13:00 PM", "7:48 am", "7:48 AM", "7:48AM", "7:48 A"}},
            {"K:mm a", new String[]{"0:30 PM", "11:59 PM", "12:00 AM"}},
            {"k:mm", new String[]{"24:00", "0:00", "25:00", "12:30"}},
            {"HH:mm:ss.SSS", new String[]{"07:48:51.123", "07:48:51.5", "07:48:51.12345", "7:8:9.1", "07:48", "07:48:51.-1"}},
            {"dd/MM/yyyy HH:mm z", new String[]{"25/09/2026 07:48 GMT", "25/09/2026 07:48 CET", "25/09/2026 07:48 CEST", "25/09/2026 07:48 EST",
                "25/09/2026 07:48 EDT", "25/09/2026 07:48 Eastern Daylight Time", "25/09/2026 07:48 GMT-05:00", "25/09/2026 07:48 -0500", "25/09/2026 07:48 XYZ",
                "25/09/2026 07:48 heure normale d’Europe centrale", "25/09/2026 07:48 utc", "25/09/2026 07:48 IST"}},
            {"G yyyy", new String[]{"AD 2026", "BC 44", "ad 2026", "ap. J.-C. 2026", "av. J.-C. 44", "CE 2026", "BCE 1", "XX 2026"}},
            {"yyyy-DDD", new String[]{"2026-268", "2026-366", "2026-0", "2024-366"}},
            {"YYYY-'W'ww-u", new String[]{"2026-W39-5", "2027-W01-1", "2026-W53-7", "2026-W00-1", "2026-W39-8", "2026-W39-0"}},
            {"yyyy-'W'ww-EEE", new String[]{"2026-W39-Fri", "2026-W01-Mon"}},
            {"MMMM yyyy", new String[]{"September 2026", "septembre 2026", "Sep 2026", "sept. 2026"}},
            {"LLLL yyyy", new String[]{"September 2026", "septembre 2026"}},
            {"EEEE", new String[]{"Friday", "vendredi", "Fri"}},
            {"yyyy MM dd", new String[]{"2026 09 25", "2026\t09\t25", "2026 09 25", "2026 09 25"}},
            {"d MMM yyyy", new String[]{"25 sept. 2026", "25 sept 2026", "25 Sep 2026", "25 septembre 2026", "25 déc. 2026", "25 DÉC. 2026"}},
            {"yyyy-MM-dd HH:mm:ss", new String[]{"2026-03-29 02:30:00", "2026-10-25 02:30:00", "2026-03-08 02:30:00", "2026-11-01 01:30:00"}},
            {"F E MMM yyyy", new String[]{"2 Fri Sep 2026", "5 Mon Sep 2026", "-1 Fri Sep 2026"}},
            {"W E MMM yyyy", new String[]{"4 Fri Sep 2026", "0 Sun Sep 2026"}},
            {"ss.SSSSSS", new String[]{"05.123456"}},
            {"EEEE, MMMM d, y 'at' h:mm:ss a zzzz", new String[]{"Friday, September 25, 2026 at 7:48:51 AM Greenwich Mean Time"}},
        };
        List<Object> out = new ArrayList<>();
        Date start = new Date(-1577923200000L); // 1920-01-01: two-digit years become 1920..2019
        for (Object[] spec : specs) {
            String p = (String) spec[0];
            for (Locale l : new Locale[]{Locale.US, Locale.FRANCE}) {
                for (String z : new String[]{"GMT", "Europe/Paris", "America/New_York"}) {
                    for (boolean lenient : new boolean[]{true, false}) {
                        SimpleDateFormat f = new SimpleDateFormat(p, l);
                        f.set2DigitYearStart(start);
                        f.setLenient(lenient);
                        List<Object> cases = new ArrayList<>();
                        for (String t : (String[]) spec[1]) {
                            f.setTimeZone(TimeZone.getTimeZone(z));
                            cases.add(parseCase(f, t, 0));
                        }
                        f.setTimeZone(TimeZone.getTimeZone(z));
                        cases.add(parseCase(f, "xx" + ((String[]) spec[1])[0], 2));
                        out.add(Json.obj("pattern", p, "locale", l.toString(), "zone", z, "lenient", lenient, "cases", cases));
                    }
                }
            }
        }
        // Default two-digit-year window (80 years back from now): years far from the boundary only.
        SimpleDateFormat f = new SimpleDateFormat("yy-MM-dd", Locale.US);
        f.setTimeZone(TimeZone.getTimeZone("GMT"));
        List<Object> cases = new ArrayList<>();
        for (String t : new String[]{"26-09-25", "99-12-31", "00-01-01", "70-01-01", "30-06-15"}) cases.add(parseCase(f, t, 0));
        out.add(Json.obj("pattern", "yy-MM-dd", "locale", "en_US", "zone", "GMT", "lenient", true, "defaultCentury", true, "cases", cases));
        Json.write(dir.resolve("date_parse.json"), out);
    }
}
