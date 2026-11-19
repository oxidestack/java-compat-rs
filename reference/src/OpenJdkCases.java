import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.RoundingMode;
import java.nio.file.Path;
import java.text.ChoiceFormat;
import java.text.DecimalFormat;
import java.text.DecimalFormatSymbols;
import java.text.MessageFormat;
import java.text.ParsePosition;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Date;
import java.util.GregorianCalendar;
import java.util.List;
import java.util.Locale;
import java.util.TimeZone;

/**
 * Classic OpenJDK regression cases for DecimalFormat, SimpleDateFormat, MessageFormat, ChoiceFormat and
 * Formatter (test/jdk/java/text/Format/..., java/util/Formatter), recreated as plain input -> output checks and
 * recorded from the JDK itself.
 */
final class OpenJdkCases {
    static final List<Object> out = new ArrayList<>();

    static Object attempt(java.util.concurrent.Callable<Object> f) {
        try {
            Object r = f.call();
            return r instanceof String ? r : Json.value(r);
        } catch (Exception e) {
            return Json.obj("error", Json.error(e));
        }
    }

    /** ["df", id, locale, pattern, [[setter, arg]...], value, result]. */
    static void df(String id, Locale l, String pattern, Object[][] setters, Object value) {
        out.add(Json.list("df", id, l.toString(), pattern, setterList(setters), Json.value(value), attempt(() -> {
            DecimalFormat f = new DecimalFormat(pattern, DecimalFormatSymbols.getInstance(l));
            for (Object[] s : setters) NumberFixtures.apply(f, (String) s[0], s[1]);
            return f.format(value);
        })));
    }

    /** ["dfparse", id, locale, pattern, [[setter, arg]...], text, result, index, errorIndex]. */
    static void dfParse(String id, Locale l, String pattern, Object[][] setters, String text) {
        DecimalFormat f = new DecimalFormat(pattern, DecimalFormatSymbols.getInstance(l));
        for (Object[] s : setters) NumberFixtures.apply(f, (String) s[0], s[1]);
        ParsePosition p = new ParsePosition(0);
        Number n = f.parse(text, p);
        out.add(Json.list("dfparse", id, l.toString(), pattern, setterList(setters), text, Json.value(n), p.getIndex(), p.getErrorIndex()));
    }

    static Object setterList(Object[][] setters) {
        List<Object> l = new ArrayList<>();
        for (Object[] s : setters) l.add(Json.list(s[0], Json.value(s[1])));
        return l;
    }

    /** ["sdf", id, locale, pattern, zone, millis, result]. */
    static void sdf(String id, Locale l, String pattern, String zone, long millis) {
        out.add(Json.list("sdf", id, l.toString(), pattern, zone, Long.toString(millis), attempt(() -> {
            SimpleDateFormat f = new SimpleDateFormat(pattern, l);
            f.setTimeZone(TimeZone.getTimeZone(zone));
            return f.format(new Date(millis));
        })));
    }

    /** ["sdfparse", id, locale, pattern, zone, lenient, twoDigitStart, text, result, index, errorIndex]. */
    static void sdfParse(String id, Locale l, String pattern, String zone, boolean lenient, long start, String text) {
        SimpleDateFormat f = new SimpleDateFormat(pattern, l);
        f.setTimeZone(TimeZone.getTimeZone(zone));
        f.setLenient(lenient);
        f.set2DigitYearStart(new Date(start));
        ParsePosition p = new ParsePosition(0);
        Date d = f.parse(text, p);
        out.add(Json.list("sdfparse", id, l.toString(), pattern, zone, lenient, Long.toString(start), text, d == null ? null : Long.toString(d.getTime()), p.getIndex(), p.getErrorIndex()));
    }

    /** ["mf", id, locale, pattern, [args], result]. */
    static void mf(String id, Locale l, String pattern, Object... args) {
        List<Object> a = new ArrayList<>();
        for (Object o : args) a.add(Json.value(o));
        out.add(Json.list("mf", id, l.toString(), pattern, a, attempt(() -> new MessageFormat(pattern, l).format(args))));
    }

    /** ["choice", id, pattern, number, result, toPattern]. */
    static void choice(String id, String pattern, double d) {
        out.add(Json.list("choice", id, pattern, Double.toString(d), attempt(() -> new ChoiceFormat(pattern).format(d)), attempt(() -> new ChoiceFormat(pattern).toPattern())));
    }

    /** ["fmt", id, locale, format, [args], result]. */
    static void fmt(String id, Locale l, String format, Object... args) {
        List<Object> a = new ArrayList<>();
        for (Object o : args) a.add(Json.value(o));
        out.add(Json.list("fmt", id, l.toString(), format, a, attempt(() -> String.format(l, format, args))));
    }

    static long utc(int y, int m, int d, int h, int mi, int s) {
        GregorianCalendar c = new GregorianCalendar(TimeZone.getTimeZone("GMT"));
        c.clear();
        c.set(y, m, d, h, mi, s);
        return c.getTimeInMillis();
    }

    static void generate(Path dir) throws Exception {
        Locale us = Locale.US;
        Object[][] none = {};
        // --- DecimalFormat (NumberRegression and friends)
        df("4088503", us, "#,##0.###", new Object[][]{{"setGroupingSize", 0}}, 123);
        df("4106658 -0.0", us, "#,##0.###", none, -0.0);
        df("4106658 -0.0001", us, "#,##0.###", none, -0.0001);
        df("4071492", us, "0.00", none, 0.00159999);
        df("4087535", us, "#,##0.###", new Object[][]{{"setMinimumIntegerDigits", 0}}, 0.0);
        df("4088161", us, "#,##0.###", new Object[][]{{"setMinimumFractionDigits", 0}, {"setMaximumFractionDigits", 16}}, 100.0);
        df("4092561", us, "#,##0.###", none, -0.001);
        df("4092480 0000", us, "0000", none, -1);
        df("4092480 0000 -5", us, "0000", none, -5.0);
        df("4106664", us, "#,##0.###", new Object[][]{{"setMultiplier", 2}}, Long.MAX_VALUE);
        df("4125885 000.00", us, "000.00", none, 12.34);
        df("4125885 percent", us, "+000.00%;-000.00%", none, 0.1234);
        df("4125885 negative percent", us, "+000.00%;-000.00%", none, -0.1234);
        df("4134034", us, "#,##0.00", none, 1.0E-5);
        df("4134034 negative", us, "#,##0.00;-#,##0.00", none, -1.0E-5);
        df("4141750", us, "#,##0.###", none, new BigInteger("123456789012345678901234567890"));
        df("4147706", us, "#,##0.0#", none, -0.0);
        df("4167494 max", us, "#,##0.###", none, Double.MAX_VALUE);
        df("4167494 min", us, "#,##0.###", none, Double.MIN_VALUE);
        df("4217661 0.001", us, "#,##0.###", none, 0.001);
        df("4217661 1.001", us, "#,##0.###", none, 1.001);
        df("4217661 0.0001", us, "#,##0.###", none, 0.0001);
        df("4243011 0.5", us, "0.", none, 0.5);
        df("4243011 1.5", us, "0.", none, 1.5);
        df("4243011 2.5", us, "0.", none, 2.5);
        df("4243108", us, "0.#E0", none, 0.0);
        df("4233840", us, "#,##0.00", none, 1.005);
        df("7131459 0.8055", us, "0.000", none, 0.8055);
        df("7131459 0.8055 HALF_UP", us, "0.000", new Object[][]{{"setRoundingMode", "HALF_UP"}}, 0.8055);
        df("7131459 1.0005", us, "0.000", none, 1.0005);
        df("8039915", us, "#,##0.###", none, 0.8055);
        df("4179818 1.2511", us, "#.#", none, 1.2511);
        df("4179818 1.2501", us, "#.#", none, 1.2501);
        df("4179818 0.9999", us, "#", none, 0.9999);
        df("engineering 12345", us, "##0.#####E0", none, 12345);
        df("engineering 123456", us, "##0.#####E0", none, 123456);
        df("engineering 0.00123", us, "##0.#####E0", none, 0.00123);
        df("scientific 0.00123", us, "00.###E0", none, 0.00123);
        df("scientific 123456", us, "0.###E0", none, 123456);
        df("fr currency", Locale.FRANCE, "#,##0.00 ¤", none, 1234.5);
        df("4290801 intl currency", us, "¤¤ #,##0.00", none, 1234.5);
        df("negative subpattern", us, "#,##0.00;(#,##0.00)", none, -1234.567);
        df("per mill", us, "#,##0‰", none, 0.1234);
        dfParse("4108738 0.1234", us, "#,##0.###", none, "0.1234");
        dfParse("4108738 1234.567", us, "#,##0.###", none, "1234.567");
        dfParse("4162852 -0", us, "#,##0.###", none, "-0");
        dfParse("4170798 -0.0 integer", us, "#,##0", new Object[][]{{"setParseIntegerOnly", true}}, "-0.0");
        dfParse("4944439 long", us, "#,##0.###", none, "123456789012345678");
        dfParse("4944439 over long", us, "#,##0.###", none, "9223372036854775808");
        dfParse("4145457 -0.0", us, "#,##0.###", none, "-0.0");
        dfParse("parse grouped", us, "#,##0.###", none, "1,234,567.89");
        dfParse("parse percent", us, "#,##0%", none, "12.5%");
        dfParse("parse stop", us, "#,##0.###", none, "12.34.56");
        dfParse("parse exponent", us, "0.###E0", none, "1.234E3");
        // --- SimpleDateFormat (DateFormatRegression / DateFormatTest)
        long d1 = utc(1997, 2, 13, 17, 59, 44);
        sdf("4052408", us, "M/d/yy h:mm a", "GMT", d1);
        sdf("4104136 's'", us, "'s'", "GMT", d1);
        sdf("4104136 'ss'", us, "'ss'", "GMT", d1);
        sdf("4151631", us, "'TO_DATE('''dd'-'MM'-'yyyy HH:mm:ss''' , ''DD-MM-YYYY HH:MI:SS'')'", "GMT", utc(1998, 5, 30, 13, 30, 0));
        sdf("4101483", us, "z", "GMT", d1);
        sdf("4210209", us, "MMM d, yyyy", "GMT", utc(2000, 0, 1, 0, 0, 0));
        sdf("4228502 H", us, "H", "GMT", utc(2000, 0, 1, 0, 0, 0));
        sdf("4228502 k", us, "k", "GMT", utc(2000, 0, 1, 0, 0, 0));
        sdf("4486735 Z", us, "Z", "America/Los_Angeles", d1);
        sdf("4486735 Z Kolkata", us, "Z", "Asia/Kolkata", d1);
        sdf("4108407 yy", us, "yy", "GMT", utc(1998, 0, 1, 0, 0, 0));
        sdf("4108407 y", us, "y", "GMT", utc(1998, 0, 1, 0, 0, 0));
        sdf("4326988", us, "yyyy-MM-dd HH:mm:ss.SSS", "GMT", utc(2001, 3, 1, 0, 0, 0) + 123);
        sdf("era BC", us, "G yyyy", "GMT", -62198755200000L);
        sdf("DST NY", us, "yyyy-MM-dd HH:mm:ss zzzz", "America/New_York", utc(2026, 6, 1, 12, 0, 0));
        sdf("DST NY winter", us, "yyyy-MM-dd HH:mm:ss zzzz", "America/New_York", utc(2026, 0, 1, 12, 0, 0));
        long start = utc(1920, 0, 1, 0, 0, 0);
        sdfParse("4059917 slash", us, "yyyy/MM/dd", "GMT", true, start, "1970/01/12");
        sdfParse("4059917 compact", us, "yyyyMMdd", "GMT", true, start, "19700112");
        sdfParse("4060212", us, "yyyy-DDD.hh:mm:ss", "GMT", true, start, "1995-040.05:01:29");
        sdfParse("4061287 lenient", us, "dd/MM/yyyy", "GMT", true, start, "35/01/1971");
        sdfParse("4061287 strict", us, "dd/MM/yyyy", "GMT", false, start, "35/01/1971");
        sdfParse("4104522", us, "hh:mm:ss a", "GMT", true, start, "09:23:08 AM");
        sdfParse("4106807", us, "yyyyMMdd '['HH:mm:ss'] 'z", "GMT", true, start, "19981230 [23:59:59] GMT");
        sdfParse("4134203", us, "dd/MM/yy HH:mm:ss", "GMT", false, start, "01/01/99 13:61:45");
        sdfParse("4151706", us, "EEEE, dd-MMM-yy HH:mm:ss z", "GMT", true, start, "Thursday, 31-Dec-98 23:00:00 GMT");
        sdfParse("4162071", us, "EEE, d MMM yyyy HH:mm:ss Z", "GMT", true, start, "Thu, 30-Jul-1999 11:51:14 GMT");
        sdfParse("4182066 09/08/99", us, "MM/dd/yy", "GMT", true, utc(1950, 0, 1, 0, 0, 0), "09/08/99");
        sdfParse("4182066 09/08/49", us, "MM/dd/yy", "GMT", true, utc(1950, 0, 1, 0, 0, 0), "09/08/49");
        sdfParse("4182066 09/08/50", us, "MM/dd/yy", "GMT", true, utc(1950, 0, 1, 0, 0, 0), "09/08/50");
        sdfParse("4182066 09/08/0050", us, "MM/dd/yy", "GMT", true, utc(1950, 0, 1, 0, 0, 0), "09/08/0050");
        sdfParse("4182066 9/8/5", us, "MM/dd/yy", "GMT", true, utc(1950, 0, 1, 0, 0, 0), "9/8/5");
        sdfParse("4210209", us, "MMM d, yyyy", "GMT", true, start, "Jan 1, 2000");
        sdfParse("5055567 k24", us, "k:mm", "GMT", true, start, "24:00");
        sdfParse("4326988", us, "yyyy-MM-dd HH:mm:ss.SSS", "GMT", true, start, "2001-04-01 00:00:00.123");
        sdfParse("DST gap", us, "yyyy-MM-dd HH:mm", "America/New_York", true, start, "2026-03-08 02:30");
        sdfParse("DST overlap", us, "yyyy-MM-dd HH:mm", "America/New_York", true, start, "2026-11-01 01:30");
        sdfParse("lenient day overflow", us, "yyyy-MM-dd", "GMT", true, start, "2026-02-30");
        sdfParse("strict day overflow", us, "yyyy-MM-dd", "GMT", false, start, "2026-02-30");
        // --- MessageFormat (MessageRegression / MessageTest)
        mf("4031438", us, "Impossible {1} has occurred -- status code is {0} and message is {2}.", 7, "failure", "bad");
        mf("4031438 quotes", us, "''{0}''", "x");
        mf("4106659", us, "{1} {2} {3} {0}", "a", "b");
        mf("4118594", us, "{0}, {0}, {0}", "a");
        mf("4105380 0", us, "{0,choice,0#There are no files|1#There is one file|1<There are {0,number,integer} files}", 0);
        mf("4105380 1", us, "{0,choice,0#There are no files|1#There is one file|1<There are {0,number,integer} files}", 1);
        mf("4105380 1273", us, "{0,choice,0#There are no files|1#There is one file|1<There are {0,number,integer} files}", 1273);
        mf("4113018 NaN", us, "{0,choice,0#zero|1#one}", Double.NaN);
        mf("4120552", us, "pattern{0,choice,0#none|1#one|2#many}", 2);
        String p4142938 = "''Vous'' {0,choice,0#n''|1#}avez sélectionné {0,choice,0#aucun|1#{0}} client{0,choice,0#s|1#|2#s} personnel{0,choice,0#s|1#|2#s}.";
        mf("4142938 0", Locale.FRANCE, p4142938, 0);
        mf("4142938 1", Locale.FRANCE, p4142938, 1);
        mf("4142938 2", Locale.FRANCE, p4142938, 2);
        mf("4169959", us, "{0}", (Object) null);
        mf("4232154", us, "{0,choice,0#{1}|1#{2}}", 1, "a", "b");
        mf("4293229", us, "'''{'0}'' '''{0}'''", "abc");
        mf("number and date", us, "On {1,date,long} at {1,time,short} the total was {0,number,currency}.", 1234.5, new Date(1790322531000L));
        mf("fr number", Locale.FRANCE, "{0,number} {0,number,percent} {0,number,integer}", 1234.567);
        mf("default Date", us, "{0}", new Date(1790322531000L));
        mf("default Number", Locale.FRANCE, "{0}", 1234567.891);
        mf("missing argument", us, "{0} {1}", "only");
        // --- ChoiceFormat
        for (double d : new double[]{-1, 0, 0.5, 1, 1.5, 2}) choice("ChoiceFormat example " + d, "0#Less than one.|1#One.|1<More than one.", d);
        for (double d : new double[]{Double.NEGATIVE_INFINITY, -1, 0, 0.5, 1, 1.5, Double.POSITIVE_INFINITY}) {
            choice("4094906 " + d, "-∞<are negative|0<are no or fraction|1#is one|1.0<is 1+|∞<are many.", d);
        }
        choice("4106660", "0#zero|1#one|2#two", 1.5);
        choice("4106661", "-1#Negative|0#Zero|1#Positive", -0.5);
        // --- Formatter (Basic tests)
        fmt("Basic %c", us, "%c", 'a');
        fmt("Basic %b null", us, "%b", (Object) null);
        fmt("Basic %.0e", us, "%.0e", 9.5);
        fmt("Basic %g small", us, "%g", 0.0001);
        fmt("Basic %g tiny", us, "%g", 0.00001);
        fmt("Basic %-10.3f|", us, "%-10.3f|", Math.PI);
        fmt("Basic %(,.2f", us, "%(,.2f", -1234567.891);
        fmt("Basic %x negative BigInteger", us, "%x", new BigInteger("-255"));
        fmt("Basic %#o", us, "%#o", 8);
        fmt("Basic %h string", us, "%h", "hello");
        fmt("Basic %e 0", us, "%e", 0.0);
        fmt("Basic %a", us, "%a", 1.0);
        fmt("Basic %.3a", us, "%.3a", Math.PI);
        fmt("Basic %a subnormal", us, "%a", Double.MIN_VALUE);
        fmt("Basic %,d fr", Locale.FRANCE, "%,d", 1234567);
        fmt("Basic %,.2f fr", Locale.FRANCE, "%,.2f", 1234567.891);
        fmt("Basic %tc", us, "%tc", 1790322531000L);
        fmt("Basic %tB fr", Locale.FRANCE, "%tB %<tA", 1790322531000L);
        fmt("Basic %s BigDecimal", us, "%s", new BigDecimal("1E+3"));
        fmt("Basic %.2f BigDecimal", us, "%.2f", new BigDecimal("1E+3"));
        fmt("Basic %e BigDecimal", us, "%e", new BigDecimal("123.456"));
        fmt("Basic %g BigDecimal", us, "%g", new BigDecimal("0.000123456"));
        fmt("Basic %010.2f", us, "%010.2f", -3.14159);
        fmt("Basic %+,d", us, "%+,d", 1234567);
        fmt("Basic % d", us, "% d", 42);
        fmt("Basic %S", us, "%S", "mixed Case");
        fmt("Basic %-5c|", us, "%-5c|", 'z');
        Json.write(dir.resolve("openjdk_cases.json"), out);
    }
}
