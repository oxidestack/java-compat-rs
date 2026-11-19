import java.math.BigDecimal;
import java.nio.file.Path;
import java.text.DecimalFormat;
import java.text.DecimalFormatSymbols;
import java.text.NumberFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.TreeSet;

/**
 * Number locale data (DecimalFormatSymbols and the four NumberFormat instances) of every available locale of the
 * languages the crate carries: the languages Keycloak 26.5.3 ships theme messages for, plus English and French. Each
 * locale is built from its language and country only (no script, no variant), as the crate's Locale is. Written to
 * locale_numbers.json; src/text/locale_numbers.rs is generated from it (reference/gen_locale_numbers.py).
 */
final class LocaleNumberFixtures {
    static final Set<String> LANGUAGES = new TreeSet<>(Arrays.asList(
        "ar", "az", "ca", "cs", "da", "de", "el", "en", "es", "eu", "fa", "fi", "fr", "hr", "hu", "it", "ja", "ka", "kk",
        "ko", "ky", "lt", "lv", "nb", "nl", "no", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "th", "tr", "uk", "zh"));

    static final Object[] VALUES = {
        0, 1, -1, 1234567L, -1234567.891, 0.5, 2.5, 1234.5, -0.25, 1e-7, 1e20, Double.NaN, Double.POSITIVE_INFINITY,
        Double.NEGATIVE_INFINITY, new BigDecimal("-1234567.8915"), new BigDecimal("0.125")
    };

    static void generate(Path dir) throws Exception {
        Set<String> tags = new TreeSet<>();
        for (Locale l : Locale.getAvailableLocales()) {
            if (LANGUAGES.contains(l.getLanguage())) {
                tags.add(l.getLanguage());
                tags.add(l.getLanguage() + "_" + l.getCountry());
            }
        }
        List<Object> out = new ArrayList<>();
        for (String t : tags) {
            if (t.endsWith("_")) continue;
            String[] p = t.split("_");
            Locale l = p.length == 1 ? new Locale(p[0]) : new Locale(p[0], p[1]);
            DecimalFormatSymbols s = DecimalFormatSymbols.getInstance(l);
            // The texts DecimalFormat expands '-', '%' and '‰' to (DecimalFormatSymbols' package-private
            // getMinusSignText(), getPercentText(), getPerMillText()).
            DecimalFormat probe = new DecimalFormat("0%;-0%", s);
            String perMillText = new DecimalFormat("0‰", s).getPositiveSuffix();
            List<Object> instances = new ArrayList<>();
            String[] kinds = {"number", "integer", "currency", "percent"};
            NumberFormat[] nfs = {NumberFormat.getNumberInstance(l), NumberFormat.getIntegerInstance(l),
                NumberFormat.getCurrencyInstance(l), NumberFormat.getPercentInstance(l)};
            for (int k = 0; k < 4; k++) {
                DecimalFormat df = (DecimalFormat) nfs[k];
                List<Object> formats = new ArrayList<>();
                for (Object v : VALUES) formats.add(Json.list(Json.value(v), NumberFixtures.fmt(df, v)));
                instances.add(Json.obj("kind", kinds[k], "pattern", df.toPattern(), "maxFrac", df.getMaximumFractionDigits(),
                        "minFrac", df.getMinimumFractionDigits(), "formats", formats));
            }
            out.add(Json.obj("locale", l.toString(), "zero", "" + s.getZeroDigit(), "grouping", "" + s.getGroupingSeparator(),
                    "decimal", "" + s.getDecimalSeparator(), "perMill", "" + s.getPerMill(), "percent", "" + s.getPercent(),
                    "digit", "" + s.getDigit(), "patternSeparator", "" + s.getPatternSeparator(), "infinity", s.getInfinity(),
                    "nan", s.getNaN(), "minus", "" + s.getMinusSign(), "currencySymbol", s.getCurrencySymbol(),
                    "intlCurrencySymbol", s.getInternationalCurrencySymbol(), "monetaryDecimal", "" + s.getMonetaryDecimalSeparator(),
                    "monetaryGrouping", "" + s.getMonetaryGroupingSeparator(), "exponent", s.getExponentSeparator(),
                    "minusText", probe.getNegativePrefix(), "percentText", probe.getPositiveSuffix(), "perMillText", perMillText,
                    "instances", instances));
        }
        Json.write(dir.resolve("locale_numbers.json"), out);
    }
}
