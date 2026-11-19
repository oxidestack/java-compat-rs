//! Java platform behaviours other ports need byte-for-byte: string escaping, number formatting, a model of
//! Java values ([`lang`], [`methods`], [`reflect`]) for engines that walk Java objects reflectively, and the
//! locale-sensitive formatting of `java.text` / `java.util` ([`text`], [`util`]) as JDK 26 does it, and
//! `java.util.regex` as JDK 21 does it ([`regex`]), and Java's EUC-JP and Big5 encoders ([`charset`]).
//!
//! ```
//! use java_compat::{double_to_string, escape_java};
//!
//! assert_eq!(double_to_string(0.1 + 0.2), "0.30000000000000004"); // Double.toString
//! assert_eq!(double_to_string(1e21), "1.0E21");
//! assert_eq!(escape_java("tab\there \"quoted\""), "tab\\there \\\"quoted\\\"");
//! ```

#![warn(missing_docs)]

pub mod charset;
mod charset_data;
mod escape;
mod html4;
pub mod jdk;
pub mod lang;
pub mod methods;
mod number;
pub mod reflect;
pub mod regex;
pub mod text;
pub mod util;

pub use escape::{escape_ecma_script, escape_java, escape_xml10, url_decode, url_encode};
pub use html4::{escape_html4, unescape_html4};
pub use number::{big_decimal_to_string, double_to_string, float_to_string};
