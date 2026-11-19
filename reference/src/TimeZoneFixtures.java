import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.TimeZone;

/** TimeZone lookup, offsets (with DST transitions) and display names. */
final class TimeZoneFixtures {
    static final String[] IDS = {"GMT", "UTC", "Europe/Paris", "America/New_York", "Europe/London", "America/Los_Angeles", "America/Chicago",
        "Asia/Tokyo", "Australia/Sydney", "Europe/Berlin", "America/Mexico_City", "Asia/Kolkata", "America/Denver", "Europe/Moscow",
        "America/Sao_Paulo", "Asia/Shanghai", "Pacific/Auckland", "America/Phoenix", "Pacific/Honolulu", "Africa/Cairo", "America/Toronto",
        "Europe/Madrid", "Europe/Rome", "Europe/Amsterdam", "Europe/Brussels", "Europe/Zurich", "Asia/Dubai", "Asia/Singapore", "Asia/Hong_Kong",
        "GMT+01:00", "GMT-5", "GMT+5:30", "GMT+0130", "GMT+24:00", "GMT+23:59", "GMT-00:00", "gmt+1", "Etc/GMT+5", "Etc/GMT-3", "Etc/UTC",
        "Foo/Bar", "EST", "PST", "CET", "Z", "UT", "MST", "HST", "EET", "WET", "MET", "CST", "IST", "JST", "ECT", "AST", "BST", "", "GMT+",
        "UTC+1", "europe/paris"};

    static final long[] INSTANTS = {0L, 1790322531000L, 1774746000000L, 1774745999999L, 1792890000000L, 1792889999999L,
        1773558000000L, 1773557999999L, 1793512800000L, 1793512799999L, -1000000000000L, -5000000000000L, -30000000000000L,
        4102444800000L, 7258118400000L, 1719792000000L, 1735689600000L};

    static void generate(Path dir) throws Exception {
        Locale[] locales = {Locale.ROOT, Locale.ENGLISH, Locale.US, Locale.FRENCH, Locale.FRANCE};
        List<Object> out = new ArrayList<>();
        for (String id : IDS) {
            TimeZone z = TimeZone.getTimeZone(id);
            List<Object> offsets = new ArrayList<>();
            for (long t : INSTANTS) offsets.add(Json.list(Long.toString(t), z.getOffset(t), z.inDaylightTime(new java.util.Date(t))));
            List<Object> names = new ArrayList<>();
            for (Locale l : locales) {
                names.add(Json.list(l.toString(), z.getDisplayName(false, TimeZone.LONG, l), z.getDisplayName(false, TimeZone.SHORT, l),
                        z.getDisplayName(true, TimeZone.LONG, l), z.getDisplayName(true, TimeZone.SHORT, l)));
            }
            out.add(Json.obj("input", id, "id", z.getID(), "class", z.getClass().getName(), "raw", z.getRawOffset(), "dstSavings", z.getDSTSavings(),
                    "usesDaylight", z.useDaylightTime(), "displayName", z.getDisplayName(), "offsets", offsets, "names", names, "toString", z.toString()));
        }
        Json.write(dir.resolve("timezone.json"), out);
    }
}
