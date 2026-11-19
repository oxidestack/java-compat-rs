# java-compat-rs

<!-- languages:begin -->
[English](README.md) · [Русский](README.ru.md) · [中文](README.zh-CN.md) · **Español**
<!-- languages:end -->
<!-- translation of README.md: efd401bf694b -->

Un port no oficial a Rust del comportamiento del JDK que otros programas necesitan byte a byte: el formateo y el
análisis de `java.text` y `java.util`, `java.util.regex`, `java.util.Calendar`/`TimeZone`/`Currency`, los juegos de
caracteres EUC-JP y Big5, el escape de cadenas y un modelo de valores de Java. Se dirige a OpenJDK 21 y 26 y se
comprueba contra salidas grabadas de ambos. Java es una marca registrada de Oracle y/o sus filiales; este proyecto no
está afiliado a Oracle ni al proyecto OpenJDK ni cuenta con su aval, y no contiene código fuente de OpenJDK traducido
([`docs/provenance.md`](docs/provenance.md), [`NOTICE`](NOTICE)).

Se escribió para [mock-server](https://github.com/oxidestack/mock-server), un MockServer en Rust que debe responder
exactamente como el original en Java, y lo comparten los demás ports de bibliotecas de Java de la misma familia.

Estado: previo a la primera versión. El crate aún no está publicado en crates.io; hasta entonces se usa desde una copia
local de este repositorio.

## Instalación

El crate se llama `java-compat-rs` y en el código se usa como `java_compat`. Aún no está publicado en crates.io; cuando
lo esté:

```toml
[dependencies]
java-compat-rs = "0.1"              # not published yet
```

Hasta entonces, depende de una copia local: `java-compat-rs = { path = "../java-compat-rs" }`.

## Inicio rápido

```rust
use java_compat::{double_to_string, escape_java};

assert_eq!(double_to_string(0.1 + 0.2), "0.30000000000000004"); // Double.toString
assert_eq!(double_to_string(1e21), "1.0E21");
assert_eq!(escape_java("tab\there \"quoted\""), "tab\\there \\\"quoted\\\"");
```

La referencia de la API estará en docs.rs cuando el crate se publique; desde una copia local la genera
`cargo doc --open`.

## Guías

[Actualización](docs/i18n/es/guide/upgrading.md): lo que cada versión pide al código que usa el crate. Este README y
las guías son también un sitio, `https://oxidestack.github.io/java-compat-rs/`, cuando el repositorio sea público.

## Compilar y probar

```sh
cargo test          # unit tests, fixture-driven tests over the recorded JDK output, doc tests
make lint           # clippy, warnings are errors
make deny           # licences, advisories and sources (cargo install cargo-deny --locked)
make doc            # the API documentation, rustdoc's warnings are errors
make public-check   # the publication gate
make site           # the documentation site (site/README.md), in a pinned node container
```

Los datos de prueba de `tests/fixtures/` están en el repositorio; grabarlos requiere un JDK (consulta `reference/` más
abajo), ejecutar las pruebas no. Para contribuir, consulta [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Qué cubre

Comportamientos que otros ports necesitan byte a byte: el escape de cadenas, el formateo de números, un modelo de
valores de Java para motores de plantillas que recorren objetos por reflexión, y el formateo dependiente de la
configuración regional de JDK 26 (`text`, `util`): `Locale`, `DecimalFormat`/`NumberFormat`,
`MessageFormat`/`ChoiceFormat`, `java.util.Formatter`, `SimpleDateFormat`/`DateFormat`, `Date`, `TimeZone`,
`GregorianCalendar`.

JDK 26 es el valor por defecto en todas partes. Donde un entorno de ejecución más antiguo difiere, el consumidor lo
elige por formato en lugar de globalmente: `SimpleDateFormat::set_jdk(Jdk::V21)` (o `with_jdk`) da el análisis
permisivo de JDK 21, en el que un literal del patrón debe coincidir exactamente con el carácter del texto, en lugar de
que en JDK 23+ cualquier separador de espacio coincida con otro (JDK-8324665; con en_US `h:mm:ss a`,
`"2:54:13 PM"` se analiza en JDK 26 y no en JDK 21). Un consumidor que replica Keycloak 26 (OpenJDK 21) lo elige;
mock-server mantiene el valor por defecto de JDK 26.

`regex` es `java.util.regex` tal como lo tiene JDK 21 (un port compartido por los ports a Rust de FreeMarker y
Velocity): `Pattern.compile` analiza la sintaxis de Java con los mensajes e índices de su `PatternSyntaxException` y
traduce el patrón a la sintaxis de fancy-regex, explicitando lo que difiere (los terminadores de línea de Java para
`.`, `^`, `$`, `\Z`; `\w`/`\d`/`\s`/`\b`, las clases POSIX y el plegado de mayúsculas solo ASCII salvo con
`UNICODE_CHARACTER_CLASS`/`UNICODE_CASE`; los nombres de propiedades de Java; `\Q...\E` reescrito primero como hace
`RemoveQEQuoting`; las intersecciones de clases; los indicadores en línea con el ámbito de Java).
`Matcher.matches`/`lookingAt`/`find`, `replaceAll`/`replaceFirst` con la sintaxis y las excepciones de
`appendExpandedReplacement`, y `Pattern.split(input, limit)` se ejecutan sobre la traducción; `methods` construye sobre
ellos `String.matches`, `replaceAll`, `replaceFirst` y `split`, y `methods::java_regex`/`expand_replacement` exponen la
traducción a quienes llaman a fancy-regex. Los vectores proceden del JDK 21 de Keycloak 26.5.3
(`tests/fixtures/jdk21/regex.json`, `reference/run.sh regex`). Las carencias conocidas se enumeran al principio de
`src/regex.rs` (bloques de Unicode, `\X`, `CANON_EQ`, sustitutos sueltos).

Los datos regionales de números (`DecimalFormatSymbols`, con los textos de varios caracteres del signo menos, el
porcentaje y el tanto por mil, y los patrones de instancia de `NumberFormat`) se incluyen para cada configuración
regional del JDK de los idiomas para los que Keycloak 26.5.3 incluye mensajes de tema, más el inglés y el francés
(`src/text/locale_numbers.rs`, generado por `reference/gen_locale_numbers.py`). `LocaleData::Full` da los de JDK 26;
`LocaleData::FullJdk21` da los del JDK 21 de Keycloak (CLDR 42), que difieren en algunas configuraciones regionales
(`ar` formatea con dígitos arábigo-índicos, `de_CH`, `no`/`nb`); `LocaleData::JavaBase` no cambia. Los demás idiomas
siguen recurriendo a root.

Los datos regionales de fechas se incluyen para las mismas configuraciones regionales del mismo modo
(`src/text/locale_dates.rs`, generado por `reference/gen_locale_dates.py` a partir de JDK 26 y del JDK 21 de
Keycloak): `DateFormatSymbols` (eras, meses, días de la semana, AM/PM), los nombres de mes independientes que usa la
`L` de `SimpleDateFormat`, los patrones de estilo de `DateFormat`, los nombres visibles de las zonas que nombra
`TimeZone` y los datos de semana de cada región (de cada JDK, `java.base` incluido). `text::has_date_data` dice si una
configuración regional formatea fechas exactamente como su JDK: no para idiomas que el JDK tiene pero el crate no
incluye, ni para `th_TH` (el JDK le da un calendario budista). Los nombres de zona siguen a una JVM que pidió primero
los nombres de root (el proveedor CLDR del JDK rellena los nombres que faltan en arrays compartidos, así que, por
ejemplo, el nombre corto de `Etc/GMT+5` en fr depende del historial).

`util::Currency` es `java.util.Currency` a partir de datos grabados (`src/util/currency_data.rs`, generado por
`reference/gen_currency.py` a partir de los tres JDK): los códigos que acepta `getInstance(String)` (se probó cada
código de tres letras) con sus dígitos fraccionarios y códigos numéricos, la moneda de cada región y
`getSymbol(locale)` en las mismas configuraciones regionales más root y un idioma desconocido; `DecimalFormatSymbols`
toma de él la moneda de su país, y `DecimalFormatSymbols::set_currency` es `setCurrency`. `charset::Charset` son los
codificadores de Java para EUC-JP (con JIS X 0212) y Big5 (sin HKSCS), cuyos homónimos WHATWG de encoding_rs codifican
de otro modo, grabados carácter a carácter (`src/charset_data.rs`, `reference/gen_charsets.py`; idénticos en los tres
JDK); la decodificación no se incluye.

`reference/` contiene los programas de Java que graban los datos de prueba (`tests/fixtures/full` de un JDK 26
completo, `tests/fixtures/javabase` del JDK 26 recortado de la imagen de MockServer, con solo los datos regionales de
`java.base`, `tests/fixtures/jdk21/{locale_numbers,locale_dates,week_data,jdk_languages,currency,charsets}.json` y
`regex.json` del JDK 21 de la imagen de Keycloak 26.5.3); vuelve a ejecutar `reference/run.sh` para regenerarlos
(`reference/run.sh localenumbers` o `reference/run.sh localedates` / `currency` / `charsets` para un solo conjunto, y
después el `reference/gen_*.py` correspondiente; `reference/run.sh regex` para los vectores de expresiones regulares).

## Obtener ayuda

Una pregunta sobre cómo funciona el crate puede ir primero a
[DeepWiki](https://deepwiki.com/oxidestack/java-compat-rs): responde a partir del código y la documentación de este
repositorio, y donde no coincide con la documentación, la documentación tiene razón. Las preguntas que no resuelva van
a [Discussions](https://github.com/oxidestack/java-compat-rs/discussions), y los errores a
[issues](https://github.com/oxidestack/java-compat-rs/issues). <!-- these three links need the public repository; it is not published yet -->

## Licencia

El código se distribuye bajo la [Apache License, Version 2.0](LICENSE) (`Apache-2.0`); [`NOTICE`](NOTICE) y
[`docs/provenance.md`](docs/provenance.md) indican de dónde proceden sus datos. La documentación —este README y las
guías de `docs/`— se distribuye bajo la licencia
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) (`CC-BY-4.0`); su texto
legal está en [`LICENSE-docs`](LICENSE-docs).
