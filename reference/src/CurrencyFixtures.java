import java.math.BigDecimal;
import java.nio.file.Path;
import java.text.DecimalFormat;
import java.text.DecimalFormatSymbols;
import java.text.NumberFormat;
import java.util.ArrayList;
import java.util.Currency;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * java.util.Currency as the JDK gives it, written to currency.json (src/util/currency_data.rs is generated from it by
 * reference/gen_currency.py):
 * - every code Currency.getInstance(String) accepts (all 26^3 upper-case three-letter codes tried; no other string is
 *   accepted), with its default fraction digits and numeric code, and getAvailableCurrencies() for comparison;
 * - Currency.getInstance(Locale) of every two-letter region it accepts (all 26^2 tried): null, or the code;
 * - getSymbol(locale) of every accepted code in every locale of the number-data languages (language and
 *   language_country), root and "xx" (a language no JDK has), where it differs from the code;
 * - DecimalFormat vectors: DecimalFormatSymbols.getInstance(locale) after setCurrency (what FreeMarker's
 *   ";; currencyCode=" does) under the locale's currency pattern and an international-symbol pattern;
 * - locales outside that set: getSymbol, DecimalFormatSymbols' currency and the currency instance.
 */
final class CurrencyFixtures {
    static final String[] FORMAT_CODES = {"USD", "EUR", "JPY", "CHF", "GBP", "BHD", "XXX", "RUB"};

    static void generate(Path dir) throws Exception {
        List<Object> codes = new ArrayList<>();
        List<String> accepted = new ArrayList<>();
        for (char a = 'A'; a <= 'Z'; a++) {
            for (char b = 'A'; b <= 'Z'; b++) {
                for (char c = 'A'; c <= 'Z'; c++) {
                    String code = "" + a + b + c;
                    try {
                        Currency cur = Currency.getInstance(code);
                        accepted.add(code);
                        codes.add(Json.list(code, cur.getDefaultFractionDigits(), cur.getNumericCode()));
                    } catch (IllegalArgumentException e) {
                        // not a code
                    }
                }
            }
        }
        Set<String> available = new TreeSet<>();
        for (Currency c : Currency.getAvailableCurrencies()) available.add(c.getCurrencyCode());
        List<Object> countries = new ArrayList<>();
        for (char a = 'A'; a <= 'Z'; a++) {
            for (char b = 'A'; b <= 'Z'; b++) {
                String cc = "" + a + b;
                try {
                    Currency c = Currency.getInstance(new Locale("", cc));
                    countries.add(Json.list(cc, c == null ? null : c.getCurrencyCode()));
                } catch (IllegalArgumentException e) {
                    // no currency data for the region
                }
            }
        }
        Set<String> tags = new TreeSet<>();
        tags.add("");
        tags.add("xx");
        for (Locale l : Locale.getAvailableLocales()) {
            if (LocaleNumberFixtures.LANGUAGES.contains(l.getLanguage())) {
                tags.add(l.getLanguage());
                if (!l.getCountry().isEmpty()) tags.add(l.getLanguage() + "_" + l.getCountry());
            }
        }
        List<Object> symbols = new ArrayList<>();
        List<Object> formats = new ArrayList<>();
        for (String t : tags) {
            Locale l = locale(t);
            Map<String, Object> differing = new TreeMap<>();
            for (String code : accepted) {
                String s = Currency.getInstance(code).getSymbol(l);
                if (!s.equals(code)) differing.put(code, s);
            }
            symbols.add(Json.obj("locale", t, "symbols", differing));
            String pattern = ((DecimalFormat) NumberFormat.getCurrencyInstance(l)).toPattern();
            for (String code : FORMAT_CODES) {
                for (String p : new String[]{pattern, "¤¤ #,##0.00;-¤¤ #,##0.00"}) {
                    DecimalFormatSymbols s = DecimalFormatSymbols.getInstance(l);
                    s.setCurrency(Currency.getInstance(code));
                    DecimalFormat df = new DecimalFormat(p, s);
                    formats.add(Json.list(t, code, p, df.format(new BigDecimal("-1234.565")), df.format(0.5)));
                }
            }
        }
        // Locales outside the recorded set: their symbols resolve through the CLDR parents (the language, else root
        // or the fallback adapter), their DecimalFormatSymbols take the country's currency.
        List<Object> fallbacks = new ArrayList<>();
        for (String t : new String[]{"de_US", "fr_JP", "ja_CH", "ru_GB", "zz", "zz_ZZ", "zz_DE", "en_AQ", "es_XK", "en_ZZ",
                "pt_AO", "ar_BH", "de_XA", "nb_XX", "it_JP"}) {
            Locale l = locale(t);
            Map<String, Object> syms = new TreeMap<>();
            for (String code : FORMAT_CODES) syms.put(code, Currency.getInstance(code).getSymbol(l));
            DecimalFormatSymbols s = DecimalFormatSymbols.getInstance(l);
            DecimalFormat cf = (DecimalFormat) NumberFormat.getCurrencyInstance(l);
            fallbacks.add(Json.obj("locale", t, "symbols", syms, "intl", s.getInternationalCurrencySymbol(),
                    "symbol", s.getCurrencySymbol(), "currencyFormat", Json.list(cf.toPattern(), cf.getMinimumFractionDigits(),
                    cf.getMaximumFractionDigits(), cf.format(-1234.5678))));
        }
        Json.writeUtf8(dir.resolve("currency.json"), Json.obj("codes", codes, "available", new ArrayList<>(available),
                "countries", countries, "symbols", symbols, "formats", formats, "fallbacks", fallbacks));
    }

    static Locale locale(String t) {
        String[] p = t.split("_");
        return t.isEmpty() ? Locale.ROOT : p.length == 1 ? new Locale(p[0]) : new Locale(p[0], p[1]);
    }
}
