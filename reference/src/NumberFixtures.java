import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.RoundingMode;
import java.nio.file.Path;
import java.text.DecimalFormat;
import java.text.DecimalFormatSymbols;
import java.text.NumberFormat;
import java.text.ParsePosition;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/** DecimalFormatSymbols, DecimalFormat and NumberFormat fixtures. */
final class NumberFixtures {
    static final Locale[] LOCALES = {Locale.ROOT, Locale.ENGLISH, Locale.US, Locale.FRENCH, Locale.FRANCE};

    static final Object[] VALUES = {
        0, 0.0, -0.0, 1, -1, 7L, 0.5, 1.5, 2.5, -2.5, 0.125, 0.135, 1.005, 2.675, 99.995, 0.0009, 0.9999, 9.9999,
        3.14159, 100.0, 1234567.891, -1234567.891, 1e-7, 1.23e-5, 123456789012345678L, Long.MAX_VALUE, Long.MIN_VALUE,
        Integer.MIN_VALUE, 1e20, 1e23, 1e300, Double.MAX_VALUE, Double.MIN_VALUE, Double.NaN, Double.POSITIVE_INFINITY,
        Double.NEGATIVE_INFINITY, 0.1f, 1.1f, new BigInteger("123456789012345678901234567890"),
        new BigInteger("-98765432109876543210"), new BigInteger("42"), new BigDecimal("1234.5678"), new BigDecimal("-0.000123456"),
        new BigDecimal("1E+5"), new BigDecimal("2.5"), new BigDecimal("0.005"), new BigDecimal("-0.0"),
        new BigDecimal("123456789012345678901234567890.123456789"), 2.3184525677263325E17, 0.30000000000000004, 1e-300,
        12345.6789, -0.0001, 5e-5, 999999.9999
    };

    static final String[] PATTERNS = {
        "#,##0.###", "0", "0.00", "#.##", "#,##0.00;(#,##0.00)", "0.###E0", "00.###E0", "##0.#####E0", "0E0", "0.0E00",
        "#%", "0.0%", "#,##0‰", "¤#,##0.00", "¤¤ #,##0.00", "'#'#", "'x''y'0", "0.0 'units'", "#,##,###",
        "000000", "#", "#.", ".00", "#,##0.0#;-#", "+0;-0", "0.00E0;(0.00E0)", "", "0.#;0.#-", "#,##0.00 ¤", "0 %",
        "##0.##E0", "#,##0.###E0", "0.###E0 m/s", "'it''s' #", "-#;+#", "#,#00.0#", "0.0##", "#0.000000000000000000000",
        "#,##0.00;-#,##0.00", "¤ #,##0.00;¤ -#,##0.00", "# ¤¤", "%#", "0.00‰", "#,##0.###;#"
    };

    static final String[] BAD_PATTERNS = {
        "#,##0.0.0", "0.00E0E0", "#,##0,", "0#", "0.#0", "'abc", ";#", "#;#;#", "%%0", "0‰%", "0E", "E0", "#.##E0", "0,", "0.0,0",
        "#,##0;", "a;b", "0;", "#,##0.00;(#)", "'", "''", "0.0E0.0"
    };

    static final String[] PARSE_PATTERNS = {"#,##0.###", "0.00%", "#,##0.00;(#,##0.00)", "¤#,##0.00", "0.###E0", "#,##0.00 ¤", "#", "0.0 'kg'"};

    static final String[] PARSE_TEXTS = {
        "123", "-123", "1,234.56", "1,234,567", "12abc", "abc", "", "-0", "0", "-0.0", "0.0", "1.5E3", "1.5E-3", "1E+3", "1E", "∞",
        "-∞", "NaN", "-NaN", "1,,234", "1,234,", ".5", "5.", "9223372036854775807", "9223372036854775808", "-9223372036854775808",
        "-9223372036854775809", "12%", "(12)", "(12.50)", "1 234,5", "1 234,5", "$12.50", "¤12.50", "12.50 €",
        "12,50 €", "0012", "1.2.3", "1,2", "E5", "1e5", "٣٤", "  12", "+12", "12 kg", "3.0 kg", "1E99999999999",
        "1E-99999999999", "123456789012345678901234567890", "0.000000000000000000000001", "50%", "-50%", "1,234.5%", "(1,234.50)x",
        "12,5", "1 234,56", "-1 234,56 €", "1.", "1,234.0", "0.1E1", "4E2%"
    };

    static Object fmt(DecimalFormat f, Object v) {
        try {
            return f.format(v);
        } catch (Exception e) {
            return Map.of("error", Json.error(e));
        }
    }

    static String tag(Locale l) {
        return l.toString();
    }

    static void generate(Path dir) throws Exception {
        // Symbols and NumberFormat instances.
        List<Object> symbols = new ArrayList<>();
        List<Object> instances = new ArrayList<>();
        for (Locale l : LOCALES) {
            DecimalFormatSymbols s = DecimalFormatSymbols.getInstance(l);
            symbols.add(Json.obj("locale", tag(l), "zero", "" + s.getZeroDigit(), "grouping", "" + s.getGroupingSeparator(),
                    "decimal", "" + s.getDecimalSeparator(), "perMill", "" + s.getPerMill(), "percent", "" + s.getPercent(),
                    "digit", "" + s.getDigit(), "patternSeparator", "" + s.getPatternSeparator(), "infinity", s.getInfinity(),
                    "nan", s.getNaN(), "minus", "" + s.getMinusSign(), "currencySymbol", s.getCurrencySymbol(),
                    "intlCurrencySymbol", s.getInternationalCurrencySymbol(), "monetaryDecimal", "" + s.getMonetaryDecimalSeparator(),
                    "monetaryGrouping", "" + s.getMonetaryGroupingSeparator(), "exponent", s.getExponentSeparator()));
            String[] kinds = {"number", "integer", "currency", "percent"};
            NumberFormat[] nfs = {NumberFormat.getNumberInstance(l), NumberFormat.getIntegerInstance(l), NumberFormat.getCurrencyInstance(l), NumberFormat.getPercentInstance(l)};
            for (int k = 0; k < 4; k++) {
                DecimalFormat df = (DecimalFormat) nfs[k];
                List<Object> formats = new ArrayList<>();
                for (Object v : VALUES) formats.add(Json.list(Json.value(v), fmt(df, v)));
                List<Object> parses = new ArrayList<>();
                for (String t : PARSE_TEXTS) parses.add(parseCase(df, t, 0));
                instances.add(Json.obj("locale", tag(l), "kind", kinds[k], "pattern", df.toPattern(), "localizedPattern", df.toLocalizedPattern(),
                        "maxFrac", df.getMaximumFractionDigits(), "minFrac", df.getMinimumFractionDigits(), "maxInt", df.getMaximumIntegerDigits(),
                        "minInt", df.getMinimumIntegerDigits(), "integerOnly", df.isParseIntegerOnly(), "formats", formats, "parses", parses));
            }
        }
        Json.write(dir.resolve("number_symbols.json"), symbols);
        Json.write(dir.resolve("number_instances.json"), instances);

        // Patterns: formatting, toPattern, errors.
        List<Object> patterns = new ArrayList<>();
        for (Locale l : new Locale[]{Locale.US, Locale.FRANCE}) {
            for (String p : PATTERNS) {
                DecimalFormat df = new DecimalFormat(p, DecimalFormatSymbols.getInstance(l));
                List<Object> formats = new ArrayList<>();
                for (Object v : VALUES) formats.add(Json.list(Json.value(v), fmt(df, v)));
                patterns.add(Json.obj("locale", tag(l), "pattern", p, "toPattern", df.toPattern(), "toLocalizedPattern", df.toLocalizedPattern(),
                        "posPrefix", df.getPositivePrefix(), "posSuffix", df.getPositiveSuffix(), "negPrefix", df.getNegativePrefix(),
                        "negSuffix", df.getNegativeSuffix(), "multiplier", df.getMultiplier(), "groupingSize", df.getGroupingSize(),
                        "groupingUsed", df.isGroupingUsed(), "decimalAlways", df.isDecimalSeparatorAlwaysShown(),
                        "maxInt", df.getMaximumIntegerDigits(), "minInt", df.getMinimumIntegerDigits(), "maxFrac", df.getMaximumFractionDigits(),
                        "minFrac", df.getMinimumFractionDigits(), "formats", formats));
            }
        }
        Json.write(dir.resolve("decimal_patterns.json"), patterns);

        List<Object> bad = new ArrayList<>();
        for (String p : BAD_PATTERNS) {
            try {
                DecimalFormat df = new DecimalFormat(p, DecimalFormatSymbols.getInstance(Locale.US));
                bad.add(Json.obj("pattern", p, "toPattern", df.toPattern()));
            } catch (Exception e) {
                bad.add(Json.obj("pattern", p, "error", Json.error(e)));
            }
        }
        // Localized patterns.
        for (String p : new String[]{"# ##0,00", "# ##0,00", "0,###E0", "# ##0,00 ¤", "0,0%"}) {
            DecimalFormat df = new DecimalFormat("0", DecimalFormatSymbols.getInstance(Locale.FRANCE));
            try {
                df.applyLocalizedPattern(p);
                bad.add(Json.obj("localizedPattern", p, "toPattern", df.toPattern(), "format", df.format(-1234.5678)));
            } catch (Exception e) {
                bad.add(Json.obj("localizedPattern", p, "error", Json.error(e)));
            }
        }
        Json.write(dir.resolve("decimal_pattern_errors.json"), bad);

        // Setters.
        List<Object> setters = new ArrayList<>();
        Object[][] ops = {
            {"setMaximumFractionDigits", 0}, {"setMaximumFractionDigits", 1}, {"setMaximumFractionDigits", 5}, {"setMaximumFractionDigits", 400},
            {"setMinimumFractionDigits", 2}, {"setMinimumFractionDigits", 5}, {"setMinimumIntegerDigits", 3}, {"setMinimumIntegerDigits", 0},
            {"setMaximumIntegerDigits", 2}, {"setMaximumIntegerDigits", 0}, {"setGroupingSize", 2}, {"setGroupingSize", 0}, {"setGroupingSize", 128},
            {"setGroupingSize", -1}, {"setGroupingUsed", false}, {"setMultiplier", -3}, {"setMultiplier", 0}, {"setMultiplier", 1000},
            {"setDecimalSeparatorAlwaysShown", true}, {"setPositivePrefix", "+"}, {"setNegativePrefix", "neg "}, {"setNegativeSuffix", "-"},
            {"setPositiveSuffix", "%p'"}, {"setRoundingMode", "UP"}, {"setRoundingMode", "DOWN"}, {"setRoundingMode", "CEILING"},
            {"setRoundingMode", "FLOOR"}, {"setRoundingMode", "HALF_UP"}, {"setRoundingMode", "HALF_DOWN"}, {"setRoundingMode", "UNNECESSARY"},
            {"setMinimumIntegerDigits", 500}, {"setMaximumIntegerDigits", 400}
        };
        for (String base : new String[]{"#,##0.###", "0.00E0", "0.0%"}) {
            for (Object[] op : ops) {
                DecimalFormat df = new DecimalFormat(base, DecimalFormatSymbols.getInstance(Locale.US));
                Object err = null;
                try {
                    apply(df, (String) op[0], op[1]);
                } catch (Exception e) {
                    err = Json.error(e);
                }
                List<Object> formats = new ArrayList<>();
                for (Object v : VALUES) formats.add(Json.list(Json.value(v), fmt(df, v)));
                setters.add(Json.obj("pattern", base, "op", op[0], "arg", Json.value(op[1]), "error", err, "toPattern", df.toPattern(),
                        "maxInt", df.getMaximumIntegerDigits(), "minInt", df.getMinimumIntegerDigits(), "maxFrac", df.getMaximumFractionDigits(),
                        "minFrac", df.getMinimumFractionDigits(), "formats", formats));
            }
        }
        Json.write(dir.resolve("decimal_setters.json"), setters);

        // Rounding modes.
        List<Object> rounding = new ArrayList<>();
        Object[] rvalues = {0.5, 1.5, 2.5, -2.5, -0.5, 0.125, 0.135, 1.005, 2.675, 1.115, -0.0001, 0.0001, 5L, 15L, 25L, -35L,
            new BigDecimal("2.5"), new BigDecimal("-1.25"), new BigDecimal("0.125"), 1.25, 1.35, 0.045, 1e-10, 12345.5, -12345.5, 0.0, -0.0};
        for (RoundingMode m : RoundingMode.values()) {
            for (String p : new String[]{"0", "0.00", "0.#E0", "#,##0.0"}) {
                DecimalFormat df = new DecimalFormat(p, DecimalFormatSymbols.getInstance(Locale.US));
                df.setRoundingMode(m);
                List<Object> formats = new ArrayList<>();
                for (Object v : rvalues) formats.add(Json.list(Json.value(v), fmt(df, v)));
                rounding.add(Json.obj("mode", m.name(), "pattern", p, "formats", formats));
            }
        }
        Json.write(dir.resolve("decimal_rounding.json"), rounding);

        // Parsing.
        List<Object> parsing = new ArrayList<>();
        for (Locale l : new Locale[]{Locale.US, Locale.FRANCE}) {
            for (String p : PARSE_PATTERNS) {
                for (int variant = 0; variant < 4; variant++) {
                    DecimalFormat df = new DecimalFormat(p, DecimalFormatSymbols.getInstance(l));
                    if (variant == 1) df.setParseIntegerOnly(true);
                    if (variant == 2) df.setParseBigDecimal(true);
                    if (variant == 3) df.setGroupingUsed(false);
                    List<Object> cases = new ArrayList<>();
                    for (String t : PARSE_TEXTS) cases.add(parseCase(df, t, 0));
                    cases.add(parseCase(df, "xx12yy", 2));
                    cases.add(parseCase(df, "12", 5));
                    parsing.add(Json.obj("locale", tag(l), "pattern", p, "variant", variant, "cases", cases));
                }
            }
        }
        // Multipliers and odd affixes.
        for (Object[] spec : new Object[][]{{"#,##0.###", 7}, {"#,##0.###", -1}, {"#,##0.###", 0}, {"#,##0.###", 100}}) {
            for (boolean big : new boolean[]{false, true}) {
                DecimalFormat df = new DecimalFormat((String) spec[0], DecimalFormatSymbols.getInstance(Locale.US));
                df.setMultiplier((Integer) spec[1]);
                df.setParseBigDecimal(big);
                List<Object> cases = new ArrayList<>();
                for (String t : new String[]{"14", "10", "-21", "0", "-0", "1.5", "1,000,000", "∞", "abc", "3.5E2"}) cases.add(parseCase(df, t, 0));
                parsing.add(Json.obj("locale", "en_US", "pattern", spec[0], "multiplier", spec[1], "bigDecimal", big, "cases", cases));
            }
        }
        Json.write(dir.resolve("decimal_parse.json"), parsing);
    }

    static Object parseCase(DecimalFormat df, String text, int index) {
        ParsePosition pos = new ParsePosition(index);
        Number n = df.parse(text, pos);
        return Json.obj("text", text, "index", index, "result", Json.value(n), "pos", pos.getIndex(), "err", pos.getErrorIndex());
    }

    static void apply(DecimalFormat df, String op, Object arg) {
        switch (op) {
            case "setMaximumFractionDigits" -> df.setMaximumFractionDigits((Integer) arg);
            case "setMinimumFractionDigits" -> df.setMinimumFractionDigits((Integer) arg);
            case "setMaximumIntegerDigits" -> df.setMaximumIntegerDigits((Integer) arg);
            case "setMinimumIntegerDigits" -> df.setMinimumIntegerDigits((Integer) arg);
            case "setGroupingSize" -> df.setGroupingSize((Integer) arg);
            case "setGroupingUsed" -> df.setGroupingUsed((Boolean) arg);
            case "setMultiplier" -> df.setMultiplier((Integer) arg);
            case "setDecimalSeparatorAlwaysShown" -> df.setDecimalSeparatorAlwaysShown((Boolean) arg);
            case "setPositivePrefix" -> df.setPositivePrefix((String) arg);
            case "setNegativePrefix" -> df.setNegativePrefix((String) arg);
            case "setPositiveSuffix" -> df.setPositiveSuffix((String) arg);
            case "setNegativeSuffix" -> df.setNegativeSuffix((String) arg);
            case "setRoundingMode" -> df.setRoundingMode(RoundingMode.valueOf((String) arg));
            case "setParseIntegerOnly" -> df.setParseIntegerOnly((Boolean) arg);
            case "setParseBigDecimal" -> df.setParseBigDecimal((Boolean) arg);
            default -> throw new IllegalStateException(op);
        }
    }
}
