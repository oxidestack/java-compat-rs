import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.TimeZone;

/** What Class.getMethods() and the class hierarchy show for the value classes templates can see. */
final class ReflectionFixtures {
    static void generate(Path dir) throws Exception {
        List<Object> out = new ArrayList<>();
        Class<?>[] classes = {Locale.class, java.util.Date.class, TimeZone.getTimeZone("Europe/Paris").getClass(), TimeZone.class,
            java.util.GregorianCalendar.class, java.text.DecimalFormat.class, java.text.SimpleDateFormat.class};
        for (Class<?> c : classes) {
            List<Object> methods = new ArrayList<>();
            Method[] ms = c.getMethods();
            Arrays.sort(ms, Comparator.comparing(Method::toString));
            for (Method m : ms) {
                List<Object> params = new ArrayList<>();
                for (Class<?> p : m.getParameterTypes()) params.add(p.getTypeName());
                methods.add(Json.list(m.getName(), params, m.getReturnType().getTypeName(), m.getDeclaringClass().getName(), Modifier.isStatic(m.getModifiers()), m.isBridge(), m.isVarArgs()));
            }
            List<Object> supers = new ArrayList<>();
            for (Class<?> s = c.getSuperclass(); s != null && s != Object.class; s = s.getSuperclass()) supers.add(s.getName());
            List<Object> interfaces = new ArrayList<>();
            collect(c, interfaces);
            out.add(Json.obj("class", c.getName(), "public", Modifier.isPublic(c.getModifiers()), "superclasses", supers, "interfaces", interfaces, "methods", methods));
        }
        Json.write(dir.resolve("reflection.json"), out);
    }

    static void collect(Class<?> c, List<Object> out) {
        for (Class<?> k = c; k != null; k = k.getSuperclass()) {
            for (Class<?> i : k.getInterfaces()) addInterface(i, out);
        }
    }

    static void addInterface(Class<?> i, List<Object> out) {
        if (!out.contains(i.getName())) out.add(i.getName());
        for (Class<?> j : i.getInterfaces()) addInterface(j, out);
    }
}
