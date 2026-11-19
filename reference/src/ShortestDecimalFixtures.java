import java.lang.reflect.Method;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;

/**
 * jdk.internal.math.FormattedFPDecimal.split(double): the shortest decimal (Double.toString's digits) plus
 * whether it is exact and whether it lies above the double, which DigitList (DecimalFormat) and Formatter use
 * in JDK 26.
 */
final class ShortestDecimalFixtures {
    static void generate(Path dir) throws Exception {
        Class<?> c = Class.forName("jdk.internal.math.FormattedFPDecimal");
        Method split = c.getDeclaredMethod("split", double.class);
        split.setAccessible(true);
        Method f = c.getMethod("getSignificand"), n = c.getMethod("getPrecision"), e = c.getMethod("getExp");
        Method exact = c.getMethod("getExact"), away = c.getMethod("getAway");
        Random r = new Random(20260925);
        List<Object> out = new ArrayList<>();
        for (int i = 0; i < 6000; i++) {
            double d = switch (i % 5) {
                case 0 -> Double.longBitsToDouble(r.nextLong() & 0x7fffffffffffffffL);
                case 1 -> r.nextInt(1000000) / Math.pow(10, r.nextInt(8));
                case 2 -> (double) r.nextLong() / (1L << r.nextInt(63));
                case 3 -> Double.longBitsToDouble(r.nextInt(5000));
                default -> Math.abs(r.nextGaussian()) * Math.pow(10, r.nextInt(40) - 20);
            };
            if (Double.isNaN(d) || Double.isInfinite(d) || d == 0) continue;
            Object fd = split.invoke(null, d);
            out.add(Json.list(Long.toHexString(Double.doubleToRawLongBits(d)), Long.toString((Long) f.invoke(fd)), (Integer) n.invoke(fd),
                    (Integer) e.invoke(fd), (Boolean) exact.invoke(fd), (Boolean) away.invoke(fd)));
        }
        for (double d : new double[]{2e23, 1e23, 5e-324, 1e-323, 9.9e-324, Double.MAX_VALUE, Double.MIN_NORMAL, 0.1, 0.5, 100.0, 1e22, 9007199254740993.0}) {
            Object fd = split.invoke(null, d);
            out.add(Json.list(Long.toHexString(Double.doubleToRawLongBits(d)), Long.toString((Long) f.invoke(fd)), (Integer) n.invoke(fd),
                    (Integer) e.invoke(fd), (Boolean) exact.invoke(fd), (Boolean) away.invoke(fd)));
        }
        Json.write(dir.resolve("shortest_decimal.json"), out);
    }
}
