import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Calendar;
import java.util.GregorianCalendar;
import java.util.List;
import java.util.Locale;
import java.util.Random;
import java.util.TimeZone;

/** GregorianCalendar: fields of instants, and random set/add/roll/clear sequences with their outcome. */
final class CalendarFixtures {
    static final String[] ZONES = {"GMT", "Europe/Paris", "America/New_York", "Australia/Sydney"};
    static final Locale[] LOCALES = {Locale.US, Locale.FRANCE, Locale.ROOT};
    static final long[] INSTANTS = {0L, 1790322531000L, 1774746000000L, 1774745999999L, 1792890000000L, 1773558000000L,
        1793512800000L, -1000000000000L, -12219292800000L, -12219292800001L, -12220000000000L, -62135596800000L, -62135596800001L,
        -100000000000000L, 253402300799999L, 1735603200000L, 1735689600000L, 1767139200000L, 1609372800000L, 1609459200000L,
        1230681600000L, 1262217600000L, 946684799999L, 951782400000L, -2208988800000L, -2208988800001L};

    static Object fields(Calendar c) {
        List<Object> f = new ArrayList<>();
        for (int i = 0; i < Calendar.FIELD_COUNT; i++) f.add(c.get(i));
        return f;
    }

    static Object state(GregorianCalendar c) {
        try {
            long t = c.getTimeInMillis();
            Object f = fields(c);
            List<Object> max = new ArrayList<>();
            for (int field : new int[]{Calendar.YEAR, Calendar.MONTH, Calendar.WEEK_OF_YEAR, Calendar.WEEK_OF_MONTH, Calendar.DAY_OF_MONTH,
                    Calendar.DAY_OF_YEAR, Calendar.DAY_OF_WEEK_IN_MONTH}) max.add(c.getActualMaximum(field));
            return Json.obj("time", Long.toString(t), "fields", f, "weekYear", c.getWeekYear(), "weeksInWeekYear", c.getWeeksInWeekYear(), "actualMax", max);
        } catch (Exception e) {
            return Json.obj("error", Json.error(e));
        }
    }

    static void generate(Path dir) throws Exception {
        List<Object> instants = new ArrayList<>();
        for (String z : ZONES) {
            for (Locale l : LOCALES) {
                for (long t : INSTANTS) {
                    GregorianCalendar c = new GregorianCalendar(TimeZone.getTimeZone(z), l);
                    c.setTimeInMillis(t);
                    instants.add(Json.obj("zone", z, "locale", l.toString(), "time", Long.toString(t), "state", state(c)));
                }
            }
        }
        Json.write(dir.resolve("calendar_instants.json"), instants);

        Random r = new Random(1234);
        int[] fields = {Calendar.ERA, Calendar.YEAR, Calendar.MONTH, Calendar.WEEK_OF_YEAR, Calendar.WEEK_OF_MONTH, Calendar.DAY_OF_MONTH,
            Calendar.DAY_OF_YEAR, Calendar.DAY_OF_WEEK, Calendar.DAY_OF_WEEK_IN_MONTH, Calendar.AM_PM, Calendar.HOUR, Calendar.HOUR_OF_DAY,
            Calendar.MINUTE, Calendar.SECOND, Calendar.MILLISECOND};
        List<Object> ops = new ArrayList<>();
        for (int n = 0; n < 1500; n++) {
            String z = ZONES[r.nextInt(ZONES.length)];
            Locale l = LOCALES[r.nextInt(LOCALES.length)];
            long start = INSTANTS[r.nextInt(INSTANTS.length)] + (r.nextBoolean() ? 0 : (long) (r.nextGaussian() * 3e10));
            GregorianCalendar c = new GregorianCalendar(TimeZone.getTimeZone(z), l);
            c.setTimeInMillis(start);
            List<Object> seq = new ArrayList<>();
            int count = 1 + r.nextInt(4);
            Object error = null;
            for (int k = 0; k < count && error == null; k++) {
                int kind = r.nextInt(n < 1200 ? 6 : 8);
                int field = fields[r.nextInt(fields.length)];
                int amount = switch (r.nextInt(4)) {
                    case 0 -> 1;
                    case 1 -> -1;
                    case 2 -> r.nextInt(30) - 15;
                    default -> r.nextInt(2000) - 1000;
                };
                try {
                    switch (kind) {
                        case 0, 1 -> { c.add(field, amount); seq.add(Json.list("add", field, amount)); }
                        case 2 -> { c.roll(field, amount); seq.add(Json.list("roll", field, amount)); }
                        case 3 -> {
                            int value = field == Calendar.YEAR ? 1900 + r.nextInt(300) : r.nextInt(40) - 5;
                            c.set(field, value);
                            seq.add(Json.list("set", field, value));
                        }
                        case 4 -> { int v = c.get(field); seq.add(Json.list("get", field, v)); }
                        case 5 -> { boolean up = r.nextBoolean(); c.roll(field, up); seq.add(Json.list("rollUp", field, up)); }
                        case 6 -> { c.clear(); seq.add(Json.list("clear")); }
                        default -> { c.setLenient(false); seq.add(Json.list("nonLenient")); }
                    }
                } catch (Exception e) {
                    error = Json.error(e);
                }
            }
            ops.add(Json.obj("zone", z, "locale", l.toString(), "start", Long.toString(start), "ops", seq, "error", error, "state", error == null ? state(c) : null));
        }
        Json.write(dir.resolve("calendar_ops.json"), ops);
    }
}
