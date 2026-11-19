import java.io.IOException;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** A tiny JSON writer for the fixtures: objects keep insertion order. */
final class Json {
    static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int i = 0; i < kv.length; i += 2) {
            m.put((String) kv[i], kv[i + 1]);
        }
        return m;
    }

    static List<Object> list(Object... items) {
        List<Object> l = new ArrayList<>();
        for (Object o : items) l.add(o);
        return l;
    }

    /** A Java value as {"t": type, "v": text}. */
    static Object value(Object v) {
        if (v == null) return obj("t", "null");
        String t = switch (v) {
            case Integer i -> "int";
            case Long l -> "long";
            case Double d -> "double";
            case Float f -> "float";
            case BigInteger b -> "bigint";
            case BigDecimal b -> "bigdec";
            case String s -> "string";
            case Boolean b -> "bool";
            case Character c -> "char";
            case java.util.Date d -> "date";
            default -> v.getClass().getName();
        };
        String text = v instanceof java.util.Date d ? Long.toString(d.getTime()) : String.valueOf(v);
        return obj("t", t, "v", text);
    }

    static String error(Throwable e) {
        return e.getClass().getName() + (e.getMessage() == null ? "" : ": " + e.getMessage());
    }

    /** Non-ASCII characters written as themselves rather than escaped (for large fixtures of locale texts). */
    private static boolean utf8;

    static void writeUtf8(Path file, Object o) throws IOException {
        utf8 = true;
        try {
            write(file, o);
        } finally {
            utf8 = false;
        }
    }

    static void write(Path file, Object o) throws IOException {
        StringBuilder sb = new StringBuilder();
        write(sb, o, 0);
        sb.append('\n');
        Files.createDirectories(file.getParent());
        Files.writeString(file, sb.toString(), StandardCharsets.UTF_8);
    }

    private static void write(StringBuilder sb, Object o, int depth) {
        if (o == null) {
            sb.append("null");
        } else if (o instanceof String s) {
            quote(sb, s);
        } else if (o instanceof Boolean || o instanceof Integer || o instanceof Long) {
            sb.append(o);
        } else if (o instanceof Map<?, ?> m) {
            sb.append('{');
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (!first) sb.append(", ");
                first = false;
                quote(sb, (String) e.getKey());
                sb.append(": ");
                write(sb, e.getValue(), depth + 1);
            }
            sb.append('}');
        } else if (o instanceof List<?> l) {
            sb.append('[');
            boolean first = true;
            for (Object item : l) {
                if (!first) sb.append(',');
                first = false;
                if (depth == 0) sb.append("\n  ");
                write(sb, item, depth + 1);
            }
            if (depth == 0 && !l.isEmpty()) sb.append('\n');
            sb.append(']');
        } else {
            quote(sb, String.valueOf(o));
        }
    }

    private static void quote(StringBuilder sb, String s) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                    if (c < 0x20 || (c > 0x7e && !utf8)) sb.append(String.format("\\u%04x", (int) c));
                    else sb.append(c);
                }
            }
        }
        sb.append('"');
    }
}
