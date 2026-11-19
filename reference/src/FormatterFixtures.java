import java.math.BigDecimal;
import java.math.BigInteger;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;
import java.util.Locale;

/** java.util.Formatter via String.format(Locale, String, Object...). */
final class FormatterFixtures {
    /** [locale, format, [args], result or {"error": ...}]. */
    static Object run(Locale l, String fmt, Object... args) {
        List<Object> a = new ArrayList<>();
        for (Object o : args) a.add(Json.value(o));
        Object result;
        try {
            result = String.format(l, fmt, args);
        } catch (Exception e) {
            result = Json.obj("error", Json.error(e));
        }
        return Json.list(l.toString(), fmt, a, result);
    }

    static void generate(Path dir) throws Exception {
        List<Object> out = new ArrayList<>();
        Locale[] locales = {Locale.US, Locale.FRANCE, Locale.ROOT, Locale.ENGLISH};
        String[] intFlags = {"", "-", "+", " ", "0", ",", "(", "#", "(,", "-#", "0,", " 0", "#0"};
        Object[] ints = {0, 42, -42, Integer.MIN_VALUE, Integer.MAX_VALUE, Long.MIN_VALUE, Long.MAX_VALUE, 1234567, -9876543210L,
            new BigInteger("123456789012345678901234567890"), new BigInteger("-98765432109876543210"), new BigInteger("0")};
        for (Locale l : new Locale[]{Locale.US, Locale.FRANCE}) {
            for (String c : new String[]{"d", "o", "x", "X"}) {
                for (String f : intFlags) {
                    for (String w : new String[]{"", "12"}) {
                        for (Object v : ints) out.add(run(l, "%" + f + w + c, v));
                    }
                }
            }
        }
        String[] floatFlags = {"", "-", "0", ",", "(", "#", "+,("};
        Object[] floats = {0.0, -0.0, 1.0, 0.5, 1.5, 2.5, 0.125, 0.135, 1.005, 123456.789, -123456.789, 1e-5, 1e-4, 9.9999e-5, 1e7, 1e21,
            1.7976931348623157e308, 4.9e-324, 2.2250738585072014E-308, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, 0.1f, 3.4e38f,
            new BigDecimal("1234.5678"), new BigDecimal("-0.000123"), new BigDecimal("1E+10"), new BigDecimal("0"), new BigDecimal("0.00"),
            new BigDecimal("9.9999"), new BigDecimal("99.995"), new BigDecimal("123456789.987654321"), 999999.5, 0.00009999, 6.02214076e23};
        for (Locale l : new Locale[]{Locale.US, Locale.FRANCE}) {
            for (String c : new String[]{"e", "E", "f", "g", "G", "a", "A"}) {
                for (String f : floatFlags) {
                    for (String w : f.isEmpty() || f.equals("-") || f.equals("0") ? new String[]{"", "14"} : new String[]{"14"}) {
                        for (String p : new String[]{"", ".0", ".2", ".17"}) {
                            for (Object v : floats) out.add(run(l, "%" + f + w + p + c, v));
                        }
                    }
                }
            }
        }
        Object[] general = {null, "text", "ünïcödé", "", true, false, 'x', 'ß', 42, -7L, 3.25, 1.0f, new BigInteger("12"), new BigDecimal("1.50"),
            new Date(1790322531000L), 0x1F600, 65};
        for (Locale l : locales) {
            for (String spec : new String[]{"%s", "%S", "%10s", "%-10s|", "%.2s", "%10.3S", "%b", "%B", "%6b", "%.2b", "%h", "%H", "%10h", "%c", "%C", "%3c", "%-3c|"}) {
                for (Object v : general) out.add(run(l, spec, v));
            }
        }
        long[] times = {0L, 1790322531007L, 1774746000000L, -62135596800001L, 1735689600000L, 43200000L, 1790337600000L, -1000000000000L};
        for (Locale l : locales) {
            for (char c : "HIklMNLQpsSTzZaAbBCdehjmyYrRcDF".toCharArray()) {
                for (String prefix : new String[]{"%t", "%T", "%12t", "%-12T"}) {
                    for (long t : times) {
                        out.add(run(l, prefix + c + "|", t));
                        if (t == 1790322531007L) out.add(run(l, prefix + c + "|", new Date(t)));
                    }
                }
            }
        }
        String[] misc = {"%%", "%n", "a%%b%nc", "%5%|", "%-5%|", "%s %s %s", "%2$s %1$s", "%1$s %<s %s", "%<s", "%s %<S", "%3$s", "%0$s",
            "%", "%q", "%-d", "%0d", "%+s", "%#s", "%#b", "%.2d", "%,e", "%#g", "%-05d", "%+ d", "%tq", "%t", "%Tz", "%-%", "%5n", "%.3c",
            "%--d", "%,x", "%(x", "%+o", "% x", "%#x", "%#o", "%#c", "%,c", "%.1t", "%2147483648s", "%.2147483648f", "%2147483648$s", "%1$", "%.f",
            "%s%", "abc", "", "%-s", "%-0s", "%(s", "%#,d", "%,+(d", "% (d", "%#0x", "%#012X", "%0#12o", "%(,.2f", "%,.3f", "%e%<f%<g%<a", "%1$d %1$x %1$o",
            "%10.4e|%-10.4e|%010.4e", "%.0e", "%#.0e", "%#.0f", "%.1g", "%#.3g", "%g", "%tY-%<tm-%<td"};
        Object[][] miscArgs = {{"a", "b", "c"}, {42}, {-42}, {3.14159}, {"str"}, {1790322531007L}, {}, {'c'}, {0x110000}, {-1}, {null}};
        for (String m : misc) {
            for (Object[] args : miscArgs) out.add(run(Locale.US, m, args));
        }
        out.add(run(Locale.US, "%d", "str"));
        out.add(run(Locale.US, "%f", 42));
        out.add(run(Locale.US, "%d", 4.5));
        out.add(run(Locale.US, "%c", 4.5));
        out.add(run(Locale.US, "%tY", "2026"));
        out.add(run(Locale.US, "%a", new BigDecimal("1.5")));
        out.add(run(Locale.US, "%c", true));
        out.add(run(Locale.US, "%x", 'c'));
        out.add(run(Locale.FRANCE, "%,.2f %,d %e %s", 1234567.891, 1234567, 1234.5, 12.5));
        out.add(run(Locale.ROOT, "%,.2f %,d %e", 1234567.891, 1234567, 1234.5));
        Json.write(dir.resolve("formatter.json"), out);
    }
}
