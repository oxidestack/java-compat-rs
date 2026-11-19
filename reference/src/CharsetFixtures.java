import java.nio.charset.Charset;
import java.nio.charset.CharsetEncoder;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import java.util.TreeSet;

/**
 * Java's encoders of the legacy CJK charsets whose WHATWG namesakes (encoding_rs) encode differently: EUC-JP (Java's
 * also writes JIS X 0212 as 0x8F + two bytes) and Big5 (Java's has no HKSCS). For each, written to charsets.json
 * (src/charset_data.rs is generated from it by reference/gen_charsets.py): the canonical name and aliases, the
 * replacement bytes, String.getBytes(charset) of every BMP char that is not mapped to '?' and not an ASCII identity,
 * the supplementary code points that are not mapped to '?', and getBytes vectors of whole strings.
 */
final class CharsetFixtures {
    static final String[] NAMES = {"EUC-JP", "Big5"};
    static final String[] VECTORS = {"a/báb?c/x;y=1 ~!*'()_-. ő€яあ😀",
        "¥‾\\~−〜‖～¦￤∥－¬￢", "一龥十卅碁穢═╔╭╴¯‾￣―—"};

    static void generate(Path dir) throws Exception {
        List<Object> out = new ArrayList<>();
        HexFormat hex = HexFormat.of();
        for (String name : NAMES) {
            if (!Charset.isSupported(name)) {
                out.add(Json.obj("name", name, "supported", false));
                continue;
            }
            Charset cs = Charset.forName(name);
            CharsetEncoder enc = cs.newEncoder();
            List<Object> map = new ArrayList<>();
            for (int c = 0; c <= 0xFFFF; c++) {
                if (Character.isSurrogate((char) c)) continue;
                byte[] b = String.valueOf((char) c).getBytes(cs);
                boolean question = b.length == 1 && b[0] == '?';
                boolean identity = c < 0x80 && b.length == 1 && b[0] == c;
                if (!identity && !(question && c != '?')) map.add(Json.list(c, hex.formatHex(b)));
            }
            List<Object> supplementary = new ArrayList<>();
            for (int c = 0x10000; c <= 0x10FFFF; c++) {
                byte[] b = new String(Character.toChars(c)).getBytes(cs);
                if (!(b.length == 1 && b[0] == '?')) supplementary.add(Json.list(c, hex.formatHex(b)));
            }
            List<Object> vectors = new ArrayList<>();
            for (String v : VECTORS) vectors.add(Json.list(v, hex.formatHex(v.getBytes(cs))));
            out.add(Json.obj("name", name, "supported", true, "canonical", cs.name(),
                    "aliases", new ArrayList<>(new TreeSet<>(cs.aliases())), "replacement", hex.formatHex(enc.replacement()),
                    "map", map, "supplementary", supplementary, "vectors", vectors));
        }
        Json.write(dir.resolve("charsets.json"), out);
    }
}
