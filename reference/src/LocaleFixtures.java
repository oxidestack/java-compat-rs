import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import org.apache.commons.lang3.LocaleUtils;

/** Locale construction, toString, hashCode, display names, and commons-lang3 LocaleUtils.toLocale. */
final class LocaleFixtures {
    @SuppressWarnings("deprecation")
    static void generate(Path dir) throws Exception {
        List<Object> out = new ArrayList<>();
        String[][] specs = {{"", "", ""}, {"en", "", ""}, {"en", "US", ""}, {"fr", "", ""}, {"fr", "FR", ""}, {"EN", "us", "Var"},
            {"", "US", ""}, {"", "", "x"}, {"", "US", "x"}, {"en", "", "x"}, {"iw", "", ""}, {"en", "US", "POSIX"}, {"en", "US", "FOO"},
            {"fr", "", "VAR"}, {"xx", "YY", ""}, {"de", "DE", ""}, {"es", "MX", ""}, {"en", "GB", ""}, {"ja", "JP", ""}, {"zh", "CN", ""},
            {"pt", "BR", ""}, {"it", "IT", ""}, {"ru", "", ""}, {"nl", "BE", ""}, {"fr", "CA", ""}, {"fr", "CH", ""}, {"en", "ZZ", ""},
            {"es", "419", ""}, {"he", "", ""}};
        Locale[] in = {Locale.ROOT, Locale.ENGLISH, Locale.US, Locale.FRENCH, Locale.FRANCE};
        for (String[] s : specs) {
            Locale l = new Locale(s[0], s[1], s[2]);
            List<Object> names = new ArrayList<>();
            for (Locale d : in) {
                names.add(Json.obj("in", d.toString(), "language", l.getDisplayLanguage(d), "country", l.getDisplayCountry(d),
                        "variant", l.getDisplayVariant(d), "name", l.getDisplayName(d)));
            }
            out.add(Json.obj("args", Json.list(s[0], s[1], s[2]), "toString", l.toString(), "language", l.getLanguage(),
                    "country", l.getCountry(), "variant", l.getVariant(), "hashCode", l.hashCode(), "languageTag", l.toLanguageTag(),
                    "displayName", l.getDisplayName(), "names", names));
        }
        Json.write(dir.resolve("locale.json"), out);

        List<Object> utils = new ArrayList<>();
        for (String s : new String[]{"en", "en_US", "fr_FR", "EN", "en_us", "e", "", "_US", "_us", "-US", "_U", "_USx", "_US_POSIX",
                "_US-POSIX", "en-US", "en_US_POSIX", "en__POSIX", "en_US_", "en#x", "eng", "engl", "en_USA", "en_123", "123", "en_US_POSIX_X",
                "zh_Hant_TW", "x", "en-US-x", "de_DE_EURO", "fr-", "en_", "EN_US", "fr_CA", "__", "_", "-", "en-", "en-us", "en_US-x"}) {
            try {
                Locale l = LocaleUtils.toLocale(s);
                utils.add(Json.obj("input", s, "toString", l.toString(), "language", l.getLanguage(), "country", l.getCountry(), "variant", l.getVariant()));
            } catch (Exception e) {
                utils.add(Json.obj("input", s, "error", Json.error(e)));
            }
        }
        Json.write(dir.resolve("locale_utils.json"), utils);
    }
}
