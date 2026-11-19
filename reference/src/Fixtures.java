import java.nio.file.Path;

/**
 * Writes the JSON fixtures the Rust tests replay. Run by run.sh twice: in a full JDK 26 (fixtures/full) and in
 * the MockServer 8.0.0 image's trimmed JDK 26, which has only java.base's locale data (fixtures/javabase).
 * Both run with default locale en, default time zone GMT. run.sh also runs the locale_numbers and locale_dates generators in Keycloak
 * 26.5.3's JDK 21 (fixtures/jdk21), whose CLDR 42 data the crate selects with LocaleData::FullJdk21, and the
 * java.util.regex vectors there too (RegexFixtures, only on request: java_compat::regex follows JDK 21).
 */
public class Fixtures {
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        String only = args.length > 1 ? args[1] : "";
        if (only.isEmpty() || only.equals("locale")) LocaleFixtures.generate(dir);
        if (only.isEmpty() || only.equals("number")) NumberFixtures.generate(dir);
        if (only.isEmpty() || only.equals("localenumbers")) LocaleNumberFixtures.generate(dir);
        if (only.isEmpty() || only.equals("localedates")) LocaleDateFixtures.generate(dir);
        if (only.isEmpty() || only.equals("fd")) ShortestDecimalFixtures.generate(dir);
        if (only.isEmpty() || only.equals("tz")) TimeZoneFixtures.generate(dir);
        if (only.isEmpty() || only.equals("calendar")) CalendarFixtures.generate(dir);
        if (only.isEmpty() || only.equals("date")) DateFixtures.generate(dir);
        if (only.isEmpty() || only.equals("zoneparse")) ZoneNameParse.generate(dir);
        if (only.isEmpty() || only.equals("formatter")) FormatterFixtures.generate(dir);
        if (only.isEmpty() || only.equals("message")) MessageFixtures.generate(dir);
        if (only.isEmpty() || only.equals("reflection")) ReflectionFixtures.generate(dir);
        if (only.isEmpty() || only.equals("openjdk")) OpenJdkCases.generate(dir);
        if (only.equals("regex")) RegexFixtures.generate(dir);
        if (only.isEmpty() || only.equals("currency")) CurrencyFixtures.generate(dir);
        if (only.isEmpty() || only.equals("charsets")) CharsetFixtures.generate(dir);
    }
}
