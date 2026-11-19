# java-compat-rs

<!-- languages:begin -->
[English](README.md) · [Русский](README.ru.md) · **中文** · [Español](README.es.md)
<!-- languages:end -->
<!-- translation of README.md: efd401bf694b -->

JDK 中其他程序需要逐字节一致的那部分行为的非官方 Rust 移植：`java.text` 和 `java.util` 的格式化与解析、
`java.util.regex`、`java.util.Calendar`/`TimeZone`/`Currency`、EUC-JP 和 Big5 字符集、字符串转义以及 Java 值模型。
它面向 OpenJDK 21 和 26，并以从两者录制的输出加以验证。Java 是 Oracle 和/或其关联公司的注册商标；本项目与 Oracle 或
OpenJDK 项目无关联，也未获其认可，且不包含任何翻译自 OpenJDK 的源码（[`docs/provenance.md`](docs/provenance.md)、
[`NOTICE`](NOTICE)）。

它是为 [mock-server](https://github.com/oxidestack/mock-server) 而写的。mock-server 是用 Rust 实现的 MockServer，
必须与 Java 原版作出完全相同的应答；同一系列的其他 Java 库移植也共用它。

状态：尚未发布。本 crate 还没有发布到 crates.io；在此之前，请从本仓库的本地检出使用它。

## 安装

crate 名为 `java-compat-rs`，在代码中以 `java_compat` 使用。它尚未发布到 crates.io；发布之后：

```toml
[dependencies]
java-compat-rs = "0.1"              # not published yet
```

在此之前，请依赖本地检出：`java-compat-rs = { path = "../java-compat-rs" }`。

## 快速开始

```rust
use java_compat::{double_to_string, escape_java};

assert_eq!(double_to_string(0.1 + 0.2), "0.30000000000000004"); // Double.toString
assert_eq!(double_to_string(1e21), "1.0E21");
assert_eq!(escape_java("tab\there \"quoted\""), "tab\\there \\\"quoted\\\"");
```

crate 发布后，API 参考将位于 docs.rs；在本地检出中可用 `cargo doc --open` 生成。

## 指南

[升级](docs/i18n/zh/guide/upgrading.md)：每个版本对使用本 crate 的代码有哪些要求。仓库公开后，本 README 和这些指南也会以
站点形式提供：`https://oxidestack.github.io/java-compat-rs/`。

## 构建与测试

```sh
cargo test          # unit tests, fixture-driven tests over the recorded JDK output, doc tests
make lint           # clippy, warnings are errors
make deny           # licences, advisories and sources (cargo install cargo-deny --locked)
make doc            # the API documentation, rustdoc's warnings are errors
make public-check   # the publication gate
make site           # the documentation site (site/README.md), in a pinned node container
```

`tests/fixtures/` 中的测试数据已提交；录制它们需要 JDK（见下文的 `reference/`），运行测试则不需要。如何参与贡献，见
[`CONTRIBUTING.md`](CONTRIBUTING.md)。

## 涵盖范围

其他移植需要逐字节一致的行为：字符串转义、数字格式化、供以反射遍历 Java 对象的模板引擎使用的 Java 值模型，以及
JDK 26 的区域敏感格式化（`text`、`util`）：`Locale`、`DecimalFormat`/`NumberFormat`、`MessageFormat`/`ChoiceFormat`、
`java.util.Formatter`、`SimpleDateFormat`/`DateFormat`、`Date`、`TimeZone`、`GregorianCalendar`。

各处默认都是 JDK 26。在较旧的运行时行为不同之处，使用者按格式而非全局地选择它：`SimpleDateFormat::set_jdk(Jdk::V21)`
（或 `with_jdk`）给出 JDK 21 的宽松解析，即模式中的字面量必须与文本中的字符完全相同，而不是 JDK 23+ 那样任意空白
分隔符可与另一个匹配（JDK-8324665；在 en_US `h:mm:ss a` 下，`"2:54:13 PM"` 在 JDK 26 上可以解析，在 JDK 21
上则不行）。与 Keycloak 26（OpenJDK 21）保持一致的使用者会选择它；mock-server 保持 JDK 26 的默认值。

`regex` 是 JDK 21 中的 `java.util.regex`（FreeMarker 和 Velocity 的 Rust 移植共用这一个移植）：`Pattern.compile`
以其 `PatternSyntaxException` 的消息和索引解析 Java 语法，并把模式翻译为 fancy-regex 的语法，明确写出不同之处
（`.`、`^`、`$`、`\Z` 的 Java 行终止符；除非设置 `UNICODE_CHARACTER_CLASS`/`UNICODE_CASE`，否则 `\w`/`\d`/`\s`/`\b`、
POSIX 类和大小写折叠仅限 ASCII；Java 属性名；按 `RemoveQEQuoting` 的方式首先改写 `\Q...\E`；类的交集；按 Java 作用域
处理的内联标志）。`Matcher.matches`/`lookingAt`/`find`、带 `appendExpandedReplacement` 语法和异常的
`replaceAll`/`replaceFirst`，以及 `Pattern.split(input, limit)` 都运行在翻译结果上；`methods` 在其上构建
`String.matches`、`replaceAll`、`replaceFirst` 和 `split`，`methods::java_regex`/`expand_replacement` 则把翻译提供给
fancy-regex 的调用方。测试向量来自 Keycloak 26.5.3 的 JDK 21（`tests/fixtures/jdk21/regex.json`、
`reference/run.sh regex`）。已知缺口列在 `src/regex.rs` 开头（Unicode 区块、`\X`、`CANON_EQ`、孤立代理项）。

数字的区域数据（`DecimalFormatSymbols`，含多字符的负号、百分号和千分号文本，以及 `NumberFormat` 实例模式）涵盖
Keycloak 26.5.3 提供主题消息的各语言在 JDK 中的每个区域，外加英语和法语（`src/text/locale_numbers.rs`，由
`reference/gen_locale_numbers.py` 生成）。`LocaleData::Full` 给出 JDK 26 的数据；`LocaleData::FullJdk21` 给出
Keycloak 的 JDK 21（CLDR 42）的数据，它在少数区域有所不同（`ar` 用阿拉伯-印度数字格式化、`de_CH`、`no`/`nb`）；
`LocaleData::JavaBase` 保持不变。其他语言仍回退到 root。

日期的区域数据以同样方式涵盖相同的区域（`src/text/locale_dates.rs`，由 `reference/gen_locale_dates.py` 从 JDK 26
和 Keycloak 的 JDK 21 生成）：`DateFormatSymbols`（纪元、月份、星期、AM/PM）、`SimpleDateFormat` 的 `L` 所用的独立
月份名、`DateFormat` 的样式模式、`TimeZone` 所命名时区的显示名，以及每个地区的星期数据（来自各个 JDK，包括
`java.base`）。`text::has_date_data` 说明某个区域格式化日期是否与其 JDK 完全一致：对于 JDK 有而本 crate 没有携带的
语言不是，`th_TH` 也不是（JDK 为它使用佛历）。时区名称遵循先请求 root 名称的 JVM（JDK 的 CLDR 提供者会把缺失的名称
填入共享数组，因此例如 fr 中 `Etc/GMT+5` 的短名称取决于历史）。

`util::Currency` 是基于录制数据的 `java.util.Currency`（`src/util/currency_data.rs`，由 `reference/gen_currency.py`
从全部三个 JDK 生成）：`getInstance(String)` 接受的代码（逐一尝试了每个三字母代码）及其小数位数和数字代码、每个地区的
货币，以及在相同区域外加 root 和一种未知语言中的 `getSymbol(locale)`；`DecimalFormatSymbols` 从中取得其国家的货币，
`DecimalFormatSymbols::set_currency` 即 `setCurrency`。`charset::Charset` 是 Java 的 EUC-JP（含 JIS X 0212）和
Big5（不含 HKSCS）编码器，encoding_rs 中同名的 WHATWG 编码器编码方式不同，因此逐字符录制（`src/charset_data.rs`、
`reference/gen_charsets.py`；三个 JDK 中相同）；不包含解码。

`reference/` 存放录制测试数据的 Java 程序（`tests/fixtures/full` 来自完整的 JDK 26，`tests/fixtures/javabase` 来自
MockServer 镜像中只有 `java.base` 区域数据的精简 JDK 26，
`tests/fixtures/jdk21/{locale_numbers,locale_dates,week_data,jdk_languages,currency,charsets}.json` 和 `regex.json`
来自 Keycloak 26.5.3 镜像的 JDK 21）；重新运行 `reference/run.sh` 即可重新生成它们（单独一组用
`reference/run.sh localenumbers` 或 `reference/run.sh localedates` / `currency` / `charsets`，然后运行对应的
`reference/gen_*.py`；正则向量用 `reference/run.sh regex`）。

## 获取帮助

关于本 crate 如何工作的问题，可以先问 [DeepWiki](https://deepwiki.com/oxidestack/java-compat-rs)：它根据本仓库的代码和
文档作答；若它与文档不一致，以文档为准。它解决不了的问题请发到
[Discussions](https://github.com/oxidestack/java-compat-rs/discussions)，缺陷请提交到
[issues](https://github.com/oxidestack/java-compat-rs/issues)。 <!-- these three links need the public repository; it is not published yet -->

## 许可证

代码采用 [Apache License, Version 2.0](LICENSE)（`Apache-2.0`）许可；[`NOTICE`](NOTICE) 和
[`docs/provenance.md`](docs/provenance.md) 说明其数据的来源。文档（本 README 及 `docs/` 下的指南）采用
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/)（`CC-BY-4.0`）许可；
其法律文本见 [`LICENSE-docs`](LICENSE-docs)。
