# java-compat-rs

<!-- languages:begin -->
[English](README.md) · **Русский** · [中文](README.zh-CN.md) · [Español](README.es.md)
<!-- languages:end -->
<!-- translation of README.md: efd401bf694b -->

Неофициальный порт на Rust того поведения JDK, которое другим программам нужно байт в байт: форматирование и разбор
`java.text` и `java.util`, `java.util.regex`, `java.util.Calendar`/`TimeZone`/`Currency`, кодировки EUC-JP и Big5,
экранирование строк и модель значений Java. Он нацелен на OpenJDK 21 и 26 и проверяется по выводу, записанному с обоих.
Java — зарегистрированный товарный знак Oracle и/или её аффилированных лиц; этот проект не связан с Oracle или
проектом OpenJDK и не одобрен ими, и в нём нет переведённого исходного кода OpenJDK
([`docs/provenance.md`](docs/provenance.md), [`NOTICE`](NOTICE)).

Он написан для [mock-server](https://github.com/oxidestack/mock-server) — MockServer на Rust, который должен отвечать
в точности как оригинал на Java, — и используется совместно другими портами Java-библиотек того же семейства.

Статус: до первого выпуска. Крейт ещё не опубликован на crates.io; пока этого не произошло, его используют из
локальной копии этого репозитория.

## Установка

Крейт называется `java-compat-rs`, а в коде используется как `java_compat`. На crates.io он ещё не опубликован; после
публикации:

```toml
[dependencies]
java-compat-rs = "0.1"              # not published yet
```

До тех пор зависьте от локальной копии: `java-compat-rs = { path = "../java-compat-rs" }`.

## Быстрый старт

```rust
use java_compat::{double_to_string, escape_java};

assert_eq!(double_to_string(0.1 + 0.2), "0.30000000000000004"); // Double.toString
assert_eq!(double_to_string(1e21), "1.0E21");
assert_eq!(escape_java("tab\there \"quoted\""), "tab\\there \\\"quoted\\\"");
```

Справочник API появится на docs.rs после публикации крейта; из локальной копии его собирает `cargo doc --open`.

## Руководства

[Обновление](docs/i18n/ru/guide/upgrading.md): что каждый выпуск требует от кода, использующего крейт. Этот README и
руководства доступны также в виде сайта, `https://oxidestack.github.io/java-compat-rs/`, после того как репозиторий
станет публичным.

## Сборка и тесты

```sh
cargo test          # unit tests, fixture-driven tests over the recorded JDK output, doc tests
make lint           # clippy, warnings are errors
make deny           # licences, advisories and sources (cargo install cargo-deny --locked)
make doc            # the API documentation, rustdoc's warnings are errors
make public-check   # the publication gate
make site           # the documentation site (site/README.md), in a pinned node container
```

Фикстуры в `tests/fixtures/` закоммичены; чтобы записать их, нужен JDK (см. `reference/` ниже), а чтобы запускать
тесты — нет. Как внести вклад, описано в [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Что покрыто

Поведение, которое другим портам нужно байт в байт: экранирование строк, форматирование чисел, модель значений Java
для шаблонизаторов, работающих через рефлексию, и зависящее от локали форматирование JDK 26 (`text`, `util`):
`Locale`, `DecimalFormat`/`NumberFormat`, `MessageFormat`/`ChoiceFormat`, `java.util.Formatter`,
`SimpleDateFormat`/`DateFormat`, `Date`, `TimeZone`, `GregorianCalendar`.

JDK 26 — значение по умолчанию везде. Там, где более старая среда выполнения ведёт себя иначе, потребитель выбирает её
для отдельного формата, а не глобально: `SimpleDateFormat::set_jdk(Jdk::V21)` (или `with_jdk`) даёт нестрогий разбор
JDK 21, при котором литерал шаблона должен точно совпадать с символом текста, вместо сопоставления в JDK 23+ любого
пробельного разделителя с любым другим (JDK-8324665; при en_US `h:mm:ss a` строка `"2:54:13 PM"` разбирается в
JDK 26 и не разбирается в JDK 21). Потребитель, повторяющий Keycloak 26 (OpenJDK 21), выбирает его; mock-server
оставляет значение по умолчанию JDK 26.

`regex` — это `java.util.regex` в том виде, в каком он есть в JDK 21 (один порт, общий для портов FreeMarker и
Velocity на Rust): `Pattern.compile` разбирает синтаксис Java с сообщениями и индексами его `PatternSyntaxException`
и переводит шаблон в синтаксис fancy-regex, явно выписывая то, что отличается (терминаторы строк Java для `.`, `^`,
`$`, `\Z`; `\w`/`\d`/`\s`/`\b` только для ASCII, классы POSIX и свёртка регистра, если не заданы
`UNICODE_CHARACTER_CLASS`/`UNICODE_CASE`; имена свойств Java; `\Q...\E`, переписываемое первым, как это делает
`RemoveQEQuoting`; пересечения классов; встроенные флаги с областью действия как в Java).
`Matcher.matches`/`lookingAt`/`find`, `replaceAll`/`replaceFirst` с синтаксисом и исключениями
`appendExpandedReplacement` и `Pattern.split(input, limit)` работают на переводе; `methods` строит на них
`String.matches`, `replaceAll`, `replaceFirst` и `split`, а `methods::java_regex`/`expand_replacement` открывают
перевод вызывающим fancy-regex. Векторы взяты из JDK 21 Keycloak 26.5.3 (`tests/fixtures/jdk21/regex.json`,
`reference/run.sh regex`). Известные пробелы перечислены в начале `src/regex.rs` (блоки Unicode, `\X`, `CANON_EQ`,
одиночные суррогаты).

Данные локалей для чисел (`DecimalFormatSymbols` с многосимвольными текстами минуса, процента и промилле, а также
шаблоны экземпляров `NumberFormat`) есть для каждой локали JDK из языков, для которых Keycloak 26.5.3 поставляет
сообщения тем, а также для английского и французского (`src/text/locale_numbers.rs`, генерируется
`reference/gen_locale_numbers.py`). `LocaleData::Full` даёт данные JDK 26; `LocaleData::FullJdk21` — JDK 21 Keycloak
(CLDR 42), которые отличаются в нескольких локалях (`ar` форматирует арабско-индийскими цифрами, `de_CH`,
`no`/`nb`); `LocaleData::JavaBase` не изменилась. Остальные языки по-прежнему откатываются к root.

Данные локалей для дат есть для тех же локалей и устроены так же (`src/text/locale_dates.rs`, генерируется
`reference/gen_locale_dates.py` из JDK 26 и JDK 21 Keycloak): `DateFormatSymbols` (эры, месяцы, дни недели, AM/PM),
отдельные названия месяцев, которые использует `L` в `SimpleDateFormat`, шаблоны стилей `DateFormat`, отображаемые
имена поясов, которые называет `TimeZone`, и данные о неделе каждого региона (из каждого JDK, включая `java.base`).
`text::has_date_data` говорит, форматирует ли локаль даты в точности как её JDK: нет — для языков, которые есть в JDK,
но не в крейте, и для `th_TH` (JDK даёт ему буддийский календарь). Имена поясов соответствуют JVM, которая сначала
запросила имена root (CLDR-провайдер JDK заполняет недостающие имена в общих массивах, поэтому, например, короткое
имя `Etc/GMT+5` во fr зависит от истории).

`util::Currency` — это `java.util.Currency` по записанным данным (`src/util/currency_data.rs`, генерируется
`reference/gen_currency.py` из всех трёх JDK): коды, которые принимает `getInstance(String)` (перебран каждый
трёхбуквенный код), с их дробными и числовыми кодами, валюта каждого региона и `getSymbol(locale)` в тех же локалях
плюс root и неизвестный язык; `DecimalFormatSymbols` берёт из него валюту своей страны, а
`DecimalFormatSymbols::set_currency` — это `setCurrency`. `charset::Charset` — кодировщики Java для EUC-JP (с JIS X
0212) и Big5 (без HKSCS), чьи тёзки WHATWG в encoding_rs кодируют иначе, записанные символ за символом
(`src/charset_data.rs`, `reference/gen_charsets.py`; одинаковы во всех трёх JDK); декодирования нет.

`reference/` содержит программы на Java, которые записывают тестовые фикстуры (`tests/fixtures/full` — из полного
JDK 26, `tests/fixtures/javabase` — из урезанного JDK 26 образа MockServer, в котором есть только данные локалей
`java.base`, `tests/fixtures/jdk21/{locale_numbers,locale_dates,week_data,jdk_languages,currency,charsets}.json` и
`regex.json` — из JDK 21 образа Keycloak 26.5.3); чтобы сгенерировать их заново, запустите `reference/run.sh`
(`reference/run.sh localenumbers` или `reference/run.sh localedates` / `currency` / `charsets` для одного набора, затем
соответствующий `reference/gen_*.py`; `reference/run.sh regex` — для векторов регулярных выражений).

## Помощь

Вопрос о том, как работает крейт, можно сначала задать [DeepWiki](https://deepwiki.com/oxidestack/java-compat-rs):
она отвечает по коду и документации этого репозитория, а там, где она расходится с документацией, права
документация. Вопросы, которые она не решает, — в
[Discussions](https://github.com/oxidestack/java-compat-rs/discussions), ошибки — в
[issues](https://github.com/oxidestack/java-compat-rs/issues). <!-- these three links need the public repository; it is not published yet -->

## Лицензия

Код распространяется по лицензии [Apache License, Version 2.0](LICENSE) (`Apache-2.0`); [`NOTICE`](NOTICE) и
[`docs/provenance.md`](docs/provenance.md) говорят, откуда взяты его данные. Документация — этот README и
руководства в `docs/` — распространяется по лицензии
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) (`CC-BY-4.0`); её
юридический текст — в [`LICENSE-docs`](LICENSE-docs).
