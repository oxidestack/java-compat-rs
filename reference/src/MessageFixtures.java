import java.math.BigDecimal;
import java.math.BigInteger;
import java.nio.file.Path;
import java.text.ChoiceFormat;
import java.text.MessageFormat;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;
import java.util.Locale;

/** MessageFormat.format and ChoiceFormat. */
final class MessageFixtures {
    static Object run(Locale l, String pattern, Object... args) {
        List<Object> a = new ArrayList<>();
        for (Object o : args) a.add(Json.value(o));
        Object result;
        Object toPattern = null;
        try {
            MessageFormat mf = new MessageFormat(pattern, l);
            toPattern = mf.toPattern();
            result = mf.format(args);
        } catch (Exception e) {
            result = Json.obj("error", Json.error(e));
        }
        return Json.list(l.toString(), pattern, a, result, toPattern);
    }

    static void generate(Path dir) throws Exception {
        List<Object> out = new ArrayList<>();
        Object[] values = {42, -1234567.891, 0.5, 1.5, new BigDecimal("1234.5678"), new BigInteger("123456789012345678901"), 7L, Double.NaN,
            Double.POSITIVE_INFINITY, "text", 'c', true, null, new Date(1790322531000L), 1790322531000L, 0};
        String[] patterns = {
            "{0}", "x{0}y", "{0,number}", "{0,number,integer}", "{0,number,percent}", "{0,number,currency}", "{0,number,#.##}", "{0,number,0000.0}",
            "{0,number,#,##0.00;(#,##0.00)}", "{0,date}", "{0,date,short}", "{0,date,medium}", "{0,date,long}", "{0,date,full}", "{0,date,yyyy-MM-dd}",
            "{0,time}", "{0,time,short}", "{0,time,long}", "{0,time,full}", "{0,time,HH:mm}", "{0,choice,0#none|1#one|1<many}",
            "{0,choice,-1#neg|0#zero|0<pos}", "{0,choice,0#no files|1#one file|1<{0,number,integer} files}", "{ 0 , number , integer }",
            "{0,NUMBER}", "{0,Number,Integer}", "{0, number, #}", "{0,number,short}", "{0,date,integer}"
        };
        for (Locale l : new Locale[]{Locale.US, Locale.FRANCE, Locale.ROOT}) {
            for (String p : patterns) {
                for (Object v : values) out.add(run(l, p, v));
            }
        }
        String[] quoting = {"It''s {0}", "'{0}' is {0}", "'{'{0}'}'", "''{0}''", "a'b{0}", "a '{0}' b", "{0} and {1} and {0}", "{1}", "{2} {0}",
            "no args", "", "'", "''", "'{", "{0}}", "}", "{", "{0", "{0,}", "{0,number,}", "{0,foo}", "{-1}", "{a}", "{10000}", "{9999}",
            "{0,number,'#'#}", "{0,choice,0#'{'0'}'|1#x}", "{0,choice,abc}", "{0,choice,1#a|0#b}", "{0,choice,#a}", "{0,choice,0#a|1<b|}",
            "{0,choice,0≤a|1≤b}", "{0,choice,-∞<neg|0#zero|∞#inf}", "{0,list}", "{0,dtf_date}", "{0,number,compact_short}",
            "{0,number,{}", "{0,date,'{'yyyy}", "{0,number,#'{'}", "Tab\t{0}\n", "{0,choice,0#{1}|1#{1,number,integer}}"};
        for (String p : quoting) {
            out.add(run(Locale.US, p, "A", 1234.5));
            out.add(run(Locale.US, p, 0, "B"));
            out.add(run(Locale.US, p));
        }
        for (double d : new double[]{-2, -1, -0.5, 0, 0.5, 1, 1.0000001, 2, 1e10, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY}) {
            out.add(run(Locale.US, "{0,choice,-1#minus one|0#zero|0<small|1#one|1<more|1e3#thousands}", d));
        }
        Json.write(dir.resolve("message_format.json"), out);

        List<Object> choice = new ArrayList<>();
        for (String p : new String[]{"0#zero|1#one|1<many", "-1#neg|0#zero|0<pos", "0#a", "1.5#x|2#y", "0#'#'|1#'|'", "0#it''s", "0≤a", "-∞<neg|0#z|∞#inf",
                " 1 #a| 2 #b", "1d#a", "0x1#a", "#a", "1#a|0#b", "a", "", "1#a|", "|1#a", "0#a|1#b|2#c"}) {
            try {
                ChoiceFormat cf = new ChoiceFormat(p);
                List<Object> formats = new ArrayList<>();
                for (double d : new double[]{-1, 0, 0.5, 1, 1.5, 2, 3, Double.NaN}) {
                    Object r;
                    try {
                        r = cf.format(d);
                    } catch (Exception e) {
                        r = Json.obj("error", Json.error(e));
                    }
                    formats.add(Json.list(Double.toString(d), r));
                }
                choice.add(Json.obj("pattern", p, "toPattern", cf.toPattern(), "formats", formats));
            } catch (Exception e) {
                choice.add(Json.obj("pattern", p, "error", Json.error(e)));
            }
        }
        Json.write(dir.resolve("choice_format.json"), choice);
    }
}
