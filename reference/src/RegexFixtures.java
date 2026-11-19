import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * java.util.regex: Pattern.compile (and its PatternSyntaxException messages), Matcher.matches/lookingAt/find with
 * groups, replaceAll/replaceFirst (appendExpandedReplacement and its exceptions), Pattern.split with limits, and the
 * String methods that delegate to them. Recorded in Keycloak 26.5.3's OpenJDK 21 (fixtures/jdk21/regex.json).
 */
final class RegexFixtures {
    static final String[] REPLACEMENTS = {
        "", "x", "[$0]", "$1", "$2", "$12", "$9", "<$1$1>", "$", "x$", "$a", "\\", "\\$1", "a\\bc", "\\\\",
        "${name}", "${year}/${m}", "${missing}", "${}", "${1a}", "${abc", "${a-b}", "$$", "$0$0",
    };
    static final int[] LIMITS = {-1, 0, 1, 2, 3};

    static Object matchGroups(Matcher m) {
        List<Object> g = new ArrayList<>();
        g.add(Json.list(m.start(), m.end()));
        for (int i = 0; i <= m.groupCount(); i++) g.add(m.group(i));
        return g;
    }

    static Object attempt(java.util.concurrent.Callable<Object> c) {
        try {
            return c.call();
        } catch (Throwable e) {
            return Json.obj("error", Json.error(e));
        }
    }

    static Object input(Pattern p, String s, boolean viaString) {
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("input", s);
        r.put("matches", attempt(() -> {
            Matcher m = p.matcher(s);
            return m.matches() ? matchGroups(m) : null;
        }));
        r.put("lookingAt", attempt(() -> p.matcher(s).lookingAt()));
        r.put("find", attempt(() -> {
            List<Object> all = new ArrayList<>();
            Matcher m = p.matcher(s);
            while (m.find()) all.add(matchGroups(m));
            return all;
        }));
        Map<String, Object> split = new LinkedHashMap<>();
        for (int limit : LIMITS) {
            split.put(Integer.toString(limit), attempt(() -> Json.list((Object[]) (viaString ? s.split(p.pattern(), limit) : p.split(s, limit)))));
        }
        r.put("split", split);
        List<Object> repl = new ArrayList<>();
        for (String rep : REPLACEMENTS) {
            repl.add(Json.list(rep,
                attempt(() -> viaString ? s.replaceAll(p.pattern(), rep) : p.matcher(s).replaceAll(rep)),
                attempt(() -> viaString ? s.replaceFirst(p.pattern(), rep) : p.matcher(s).replaceFirst(rep))));
        }
        r.put("replace", repl);
        if (viaString) r.put("stringMatches", attempt(() -> s.matches(p.pattern())));
        return r;
    }

    static void add(List<Object> out, String pattern, int flags, String... inputs) {
        Map<String, Object> c = new LinkedHashMap<>();
        c.put("pattern", pattern);
        c.put("flags", flags);
        Pattern p;
        try {
            p = Pattern.compile(pattern, flags);
        } catch (Throwable e) {
            c.put("error", Json.error(e));
            out.add(c);
            return;
        }
        c.put("groupCount", p.matcher("").groupCount());
        List<Object> in = new ArrayList<>();
        for (String s : inputs) in.add(input(p, s, flags == 0));
        c.put("inputs", in);
        out.add(c);
    }

    static final String[] TEXT = {"", "abc", "a1 b2_c3", "Hello World", "a\nb\r\nc\rd\u0085e f g", "x\n", "x\r\n",
        "héllo wörld ١٢", "STRASSE straße Été", "a,b,,c,,", ",a,,b", "2026-10-15 1999-01-02"};

    static void generate(Path dir) throws Exception {
        List<Object> out = new ArrayList<>();
        int I = Pattern.CASE_INSENSITIVE, M = Pattern.MULTILINE, S = Pattern.DOTALL, D = Pattern.UNIX_LINES,
            U = Pattern.UNICODE_CASE, X = Pattern.COMMENTS, UC = Pattern.UNICODE_CHARACTER_CLASS;
        // Line terminators: . ^ $ \Z \z under each flag combination.
        for (int f : new int[]{0, M, S, D, M | D, S | D}) {
            add(out, "a.c", f, "abc", "a\nc", "a\rc", "a\u0085c", "a c", "a c", "aéc");
            add(out, "^\\w+$", f, TEXT);
            add(out, ".+", f, "ab\ncd\r\nef\u0085gh\r");
            add(out, "^", f, "a\nb\r\nc\rd\n");
            add(out, "$", f, "a\nb\r\nc\rd\n", "x\r\n", "x\r", "x ");
            add(out, "x\\Z", f, "x", "x\n", "x\r\n", "x\r", "x\n\n", "x\u0085");
            add(out, "x\\z", f, "x", "x\n");
        }
        // Predefined classes, ASCII-only unless UNICODE_CHARACTER_CLASS.
        for (int f : new int[]{0, UC, I}) {
            for (String p : new String[]{"\\w+", "\\W+", "\\d+", "\\D", "\\s+", "\\S+", "\\b", "\\B", "\\bw", "\\h", "\\v", "\\R",
                "\\p{Lower}+", "\\p{Upper}+", "\\p{Alpha}+", "\\p{Punct}", "\\p{Alnum}+", "\\p{XDigit}+", "\\p{Space}", "\\p{Graph}+",
                "\\p{javaLowerCase}+", "\\p{IsAlphabetic}+", "\\p{L}+", "\\p{Lu}", "\\pL+", "\\PL+", "\\p{IsLatin}+", "\\p{sc=Greek}+",
                "\\p{gc=Nd}+", "\\p{IsDigit}+", "\\p{IsLowercase}+", "\\p{IsPunctuation}", "\\p{all}", "\\p{ASCII}+", "\\p{L1}+"}) {
                add(out, p, f, "héllo wörld_42 ١٢ αβγ ÉTÉ a b c\n\r\u000b!?");
            }
        }
        // Case folding: ASCII-only without UNICODE_CASE.
        for (int f : new int[]{I, I | U, U, 0}) {
            add(out, "straße|été|abc", f, "STRASSE straße ÉTÉ été ABC aBc");
            add(out, "[a-eà-ÿ]+", f, "ABCXYZ ÀÉ àé");
            add(out, "(a)\\1", f, "aA Aa aa");
            add(out, "\\p{Lower}+|\\p{Lu}", f, "abCD");
        }
        add(out, "(?i)abc(?-i)abc", 0, "ABCabc ABCABC abcabc");
        add(out, "(?i:x)y", 0, "XY Xy xy");
        add(out, "a(?i)b|c", 0, "aB C c AB");
        add(out, "(?iu)é", 0, "Éé");
        add(out, "(?U)\\w+", 0, "héllo ١");
        add(out, "(?s).+", 0, "a\nb");
        add(out, "(?m)^b", 0, "a\nb");
        add(out, "(?d).$", 0, "a\r\nb\r");
        // Classes: union, intersection, negation, ranges, escapes in classes.
        for (String p : new String[]{"[a-z&&[^aeiou]]+", "[abc[x-z]]+", "[^a-c]+", "[\\w&&\\D]+", "[a-z&&def]+", "[^abc&&b-d]+",
            "[\\Qa-c\\E]+", "[\\[\\]]+", "[-a]+", "[a-]+", "[\\x41-\\x43]+", "[\\u0061\\t]+", "[\\0141]+", "[\\p{L}&&[^\\p{Lu}]]+",
            "[\\d\\s]+", "[.]", "[$^]+", "[a&&b]", "[a-c&&]", "[&&a]", "[a&b]+", "[\\^]"}) {
            add(out, p, 0, "abcdefxyz ABC 123 [x]-a&^$.\tb");
        }
        add(out, "(?x) [a b] + # comment\n c", 0, "a bbc ab c");
        add(out, "(?x)a\\ b", 0, "a b ab");
        add(out, "[a b]+", X, "a b ab");
        // Escapes, quoting, quantifiers.
        for (String p : new String[]{"\\Qa.b\\E+", "\\Qa.b", "a\\Q\\E+", "\\x41", "\\x{1F600}", "\\u0041\\u0062", "\\0101", "\\011",
            "\\cA", "\\e", "\\t", "\\a", "\\f", "\\.", "\\-", "a{2}", "a{2,}", "a{1,2}?", "a{2}+", "a++a", "(?>a+)a", "a*?b", "x*",
            "(a)?b", "(a)|(b)", "(?:a|b)+", "(?=a)\\w", "(?!a)\\w", "(?<=a)b", "(?<!a)b", "(?<=a{1,3})b", "a|", "|", "", "\\G\\w"}) {
            add(out, p, 0, "aab aaab AB \u0001\u001b\t\u0007\f\t.- ba b xyz", "", "aaa");
        }
        // Supplementary characters, never with a pattern that matches empty (Java would split the surrogate pair).
        add(out, "\\x{1F600}|.", 0, "a😀b");
        add(out, "[\\x{1F600}-\\x{1F64F}]+", 0, "a😀🙏b");
        // Groups, named groups, back references.
        add(out, "(?<year>\\d{4})-(?<m>\\d\\d)", 0, "2026-10-15 1999-01-02", "none");
        add(out, "(?<name>a)(b)?", 0, "a ab");
        add(out, "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)", 0, "abcdefghijk");
        add(out, "(a)\\11", 0, "aa1 aaa");
        add(out, "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)\\11", 0, "abcdefghijj abcdefghija1");
        add(out, "(a)\\2", 0, "aa");
        add(out, "\\1(a)", 0, "aa a");
        add(out, "(?<n>x)\\k<n>", 0, "xx x");
        add(out, "(\\w)\\1", 0, "hello aabb");
        // Splitting.
        for (String p : new String[]{",", "\\.", "|", "\\|", "\\s*,\\s*", "", "(?=b)", "x*", ",+", "\\d"}) {
            add(out, p, 0, "a,b,,c,,", ",a", "", "a.b|c", "abcb", "a1b22c", "  , x ,y");
        }
        // Syntax errors.
        for (String p : new String[]{"(", ")", "a)", "(a", "[a", "[", "[]", "[]a]", "[^]", "*a", "a**", "+", "?", "a{", "a{x}", "{1}",
            "a{3,2}", "a{1", "a{,3}", "\\", "a\\", "(?<1a>x)", "(?<a>x)(?<a>y)", "(?<a-b>x)", "\\k<n>", "\\kn", "(?z)", "(?", "(?<",
            "(?$x)", "\\x{110000}", "\\x{", "\\x{12", "\\xg", "\\x4", "\\u12", "\\0", "\\08", "\\c", "\\p{Foo}", "\\p{", "\\p",
            "\\p{IsFoo}", "\\p{sc=Foo}", "\\p{gc=Xx}", "\\p{foo=bar}", "[b-a]", "[a-\\d]", "\\y", "\\i", "[\\8]", "x{2147483648}",
            "(?<=a+)b", "(?x)[ ]", "[a&&]", "a|*"}) {
            add(out, p, 0, "a");
        }
        Json.write(dir.resolve("regex.json"), out);
    }
}
