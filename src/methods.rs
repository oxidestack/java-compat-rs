//! Public methods of the JDK classes [`JValue`] models, as reflection would find and invoke them.

use std::sync::Arc;

use crate::reflect::JType;
use crate::regex::Pattern;

use crate::lang::{JArray, JError, JList, JMap, JObject, JValue, ListKind, MapEntry, MapKind, java_equals, java_hash_code, string_hash};

/// `Some(Ok(Some(v)))` for a value, `Some(Ok(None))` for `void`, `Some(Err(e))` for a thrown exception, `None`
/// when the class has no public method `name` taking these arguments.
pub type Invocation = Option<Result<Option<JValue>, JError>>;

fn ok(v: JValue) -> Invocation {
    Some(Ok(Some(v)))
}

fn err(class: &str, message: impl Into<String>) -> Invocation {
    Some(Err(JError::new(class, message)))
}

fn int(v: usize) -> JValue {
    JValue::Int(v as i32)
}

/// An `int` argument (any integral boxed value).
pub fn as_int(v: &JValue) -> Option<i64> {
    match v {
        JValue::Byte(b) => Some(*b as i64),
        JValue::Short(s) => Some(*s as i64),
        JValue::Int(i) => Some(*i as i64),
        JValue::Long(l) => Some(*l),
        JValue::Char(c) => Some(*c as i64),
        _ => None,
    }
}

/// Invokes a public method on a value (built-in JDK class or [`JObject`]).
pub fn invoke(target: &JValue, name: &str, args: &[JValue]) -> Invocation {
    let result = match target {
        JValue::Str(s) => string(s, name, args),
        JValue::Bool(b) => boolean(*b, name, args),
        JValue::Byte(_) | JValue::Short(_) | JValue::Int(_) | JValue::Long(_) | JValue::Float(_) | JValue::Double(_) | JValue::BigInteger(_) | JValue::BigDecimal(_) => {
            number(target, name, args)
        }
        JValue::Char(c) => character(*c, name, args),
        JValue::List(l) => list(l, name, args),
        JValue::Map(m) => map(m, name, args),
        JValue::Array(a) => array(a, name, args),
        JValue::Object(o) => o.invoke(name, args),
        JValue::Null => None,
    };
    result.or_else(|| object_method(target, name, args))
}

/// `java.lang.Object`'s own public methods.
fn object_method(target: &JValue, name: &str, args: &[JValue]) -> Invocation {
    match (name, args) {
        ("toString", []) => ok(JValue::str(target.to_java_string())),
        ("hashCode", []) => ok(JValue::Int(java_hash_code(target))),
        ("equals", [other]) => ok(JValue::Bool(java_equals(target, other))),
        _ => None,
    }
}

fn chars(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_units(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn index_of(haystack: &[u16], needle: &[u16], from: i64) -> i64 {
    let from = from.max(0) as usize;
    if needle.is_empty() {
        return from.min(haystack.len()) as i64;
    }
    (from..=haystack.len().saturating_sub(needle.len())).find(|&i| haystack.get(i..i + needle.len()) == Some(needle)).map(|i| i as i64).unwrap_or(-1)
}

fn last_index_of(haystack: &[u16], needle: &[u16], from: i64) -> i64 {
    if from < 0 || needle.len() > haystack.len() {
        return -1;
    }
    let start = (from as usize).min(haystack.len() - needle.len());
    (0..=start).rev().find(|&i| &haystack[i..i + needle.len()] == needle).map(|i| i as i64).unwrap_or(-1)
}

/// A `String`/`CharSequence` or `char` argument as text.
fn text(v: &JValue) -> Option<String> {
    match v {
        JValue::Str(s) => Some(s.to_string()),
        JValue::Char(c) => Some(c.to_string()),
        _ => None,
    }
}

fn out_of_bounds(message: String) -> Invocation {
    err("java.lang.StringIndexOutOfBoundsException", message)
}

/// Java's `Pattern.compile(pattern)` as a fancy-regex [`fancy_regex::Regex`]: the pattern translated by
/// [`crate::regex`] (Java's syntax, errors as `PatternSyntaxException`), so searching it behaves as `Matcher.find`
/// does. Prefer [`crate::regex::Pattern`], which also has `matches`, `split` and replacements.
pub fn java_regex(pattern: &str) -> Result<fancy_regex::Regex, JError> {
    Ok(Pattern::get(pattern, 0)?.regex().clone())
}

/// `Matcher.appendReplacement` syntax (JDK 21's `appendExpandedReplacement`): `$n` / `${name}` group references, `\`
/// escapes, and their `IllegalArgumentException` / `IndexOutOfBoundsException`s. From fancy-regex captures alone a
/// `${name}` of a group the pattern lacks cannot be told from one that took no part, so it yields "" instead of
/// Java's `No group with name {name}`; [`crate::regex::Pattern::append_replacement`] has the exact behaviour.
pub fn expand_replacement(caps: &fancy_regex::Captures<'_, str>, replacement: &str) -> Result<String, JError> {
    let mut out = String::new();
    crate::regex::expand(
        replacement,
        caps.len().saturating_sub(1),
        |name| Ok(caps.name(name).map(|m| m.as_str())),
        |i| caps.get(i).map(|m| m.as_str()),
        &mut out,
    )?;
    Ok(out)
}

/// `String.replaceAll` (`all`) / `replaceFirst`.
fn replace_regex(s: &str, pattern: &str, replacement: &str, all: bool) -> Result<String, JError> {
    Pattern::get(pattern, 0)?.replace(s, replacement, all)
}

/// `String.split(regex, limit)`, including the trailing-empty-string rules.
pub fn split(s: &str, pattern: &str, limit: i64) -> Result<Vec<String>, JError> {
    Pattern::get(pattern, 0)?.split(s, limit.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

/// `String.matches(regex)`.
fn string_matches(s: &str, pattern: &str) -> Result<bool, JError> {
    Ok(Pattern::get(pattern, 0)?.matches(s)?.is_some())
}

fn string_array(items: Vec<String>) -> JValue {
    JValue::Array(Arc::new(JArray::new(JType::string(), items.into_iter().map(JValue::str).collect())))
}

fn string(s: &Arc<str>, name: &str, args: &[JValue]) -> Invocation {
    let units = || chars(s);
    match (name, args) {
        ("length", []) => ok(int(units().len())),
        ("isEmpty", []) => ok(JValue::Bool(s.is_empty())),
        ("isBlank", []) => ok(JValue::Bool(s.chars().all(char::is_whitespace))),
        ("toString" | "intern" | "strip" | "stripLeading" | "stripTrailing" | "trim", []) => {
            let v = match name {
                "strip" => s.trim().to_string(),
                "stripLeading" => s.trim_start().to_string(),
                "stripTrailing" => s.trim_end().to_string(),
                // `trim` removes every char <= U+0020.
                "trim" => s.trim_matches(|c: char| c <= ' ').to_string(),
                _ => s.to_string(),
            };
            ok(JValue::str(v))
        }
        ("hashCode", []) => ok(JValue::Int(string_hash(s))),
        ("toUpperCase", []) => ok(JValue::str(s.to_uppercase())),
        ("toLowerCase", []) => ok(JValue::str(s.to_lowercase())),
        ("toCharArray", []) => ok(JValue::Array(Arc::new(JArray::new(JType::Char, s.chars().map(JValue::Char).collect())))),
        ("getBytes", []) => ok(JValue::Array(Arc::new(JArray::new(JType::Byte, s.bytes().map(|b| JValue::Byte(b as i8)).collect())))),
        ("charAt", [i]) => {
            let i = as_int(i)?;
            let u = units();
            match u.get(i as usize).filter(|_| i >= 0) {
                Some(c) => ok(char::from_u32(*c as u32).map(JValue::Char).unwrap_or(JValue::Char('\u{fffd}'))),
                None => out_of_bounds(format!("Index {i} out of bounds for length {}", u.len())),
            }
        }
        ("substring", [b]) | ("substring", [b, _]) => {
            let u = units();
            let begin = as_int(b)?;
            let end = match args.get(1) {
                Some(e) => as_int(e)?,
                None => u.len() as i64,
            };
            if begin < 0 || end > u.len() as i64 || begin > end {
                return out_of_bounds(format!("begin {begin}, end {end}, length {}", u.len()));
            }
            ok(JValue::str(from_units(&u[begin as usize..end as usize])))
        }
        ("indexOf", [needle]) | ("indexOf", [needle, _]) => {
            let from = args.get(1).and_then(as_int).unwrap_or(0);
            let needle = match needle {
                JValue::Int(c) => char::from_u32(*c as u32)?.to_string(),
                other => text(other)?,
            };
            ok(JValue::Int(index_of(&units(), &chars(&needle), from) as i32))
        }
        ("lastIndexOf", [needle]) | ("lastIndexOf", [needle, _]) => {
            let u = units();
            let from = args.get(1).and_then(as_int).unwrap_or(u.len() as i64);
            let needle = match needle {
                JValue::Int(c) => char::from_u32(*c as u32)?.to_string(),
                other => text(other)?,
            };
            ok(JValue::Int(last_index_of(&u, &chars(&needle), from) as i32))
        }
        ("contains", [t]) => ok(JValue::Bool(s.contains(text(t)?.as_str()))),
        ("startsWith", [p]) => ok(JValue::Bool(s.starts_with(text(p)?.as_str()))),
        ("startsWith", [p, offset]) => {
            let u = units();
            let offset = as_int(offset)?;
            let p = chars(&text(p)?);
            ok(JValue::Bool(offset >= 0 && u.get(offset as usize..).is_some_and(|rest| rest.starts_with(&p))))
        }
        ("endsWith", [p]) => ok(JValue::Bool(s.ends_with(text(p)?.as_str()))),
        ("equals", [o]) => ok(JValue::Bool(o.as_str() == Some(s))),
        ("equalsIgnoreCase", [o]) => ok(JValue::Bool(o.as_str().is_some_and(|o| o.to_lowercase() == s.to_lowercase()))),
        ("contentEquals", [o]) => ok(JValue::Bool(text(o).as_deref() == Some(s))),
        ("compareTo", [o]) => {
            let o = chars(o.as_str()?);
            let u = units();
            let diff = u.iter().zip(&o).find(|(a, b)| a != b).map(|(a, b)| *a as i32 - *b as i32).unwrap_or(u.len() as i32 - o.len() as i32);
            ok(JValue::Int(diff))
        }
        ("compareToIgnoreCase", [o]) => {
            let (a, b) = (s.to_lowercase(), o.as_str()?.to_lowercase());
            let (a, b) = (chars(&a), chars(&b));
            let diff = a.iter().zip(&b).find(|(x, y)| x != y).map(|(x, y)| *x as i32 - *y as i32).unwrap_or(a.len() as i32 - b.len() as i32);
            ok(JValue::Int(diff))
        }
        ("concat", [o]) => ok(JValue::str(format!("{s}{}", o.as_str()?))),
        ("repeat", [n]) => {
            let n = as_int(n)?;
            if n < 0 {
                return err("java.lang.IllegalArgumentException", format!("count is negative: {n}"));
            }
            ok(JValue::str(s.repeat(n as usize)))
        }
        ("replace", [a, b]) => ok(JValue::str(s.replace(text(a)?.as_str(), text(b)?.as_str()))),
        ("replaceAll", [p, r]) => Some(replace_regex(s, p.as_str()?, r.as_str()?, true).map(|v| Some(JValue::str(v)))),
        ("replaceFirst", [p, r]) => Some(replace_regex(s, p.as_str()?, r.as_str()?, false).map(|v| Some(JValue::str(v)))),
        ("matches", [p]) => Some(string_matches(s, p.as_str()?).map(|b| Some(JValue::Bool(b)))),
        ("split", [p]) => Some(split(s, p.as_str()?, 0).map(|v| Some(string_array(v)))),
        ("split", [p, limit]) => Some(split(s, p.as_str()?, as_int(limit)?).map(|v| Some(string_array(v)))),
        ("codePointAt", [i]) => {
            let i = as_int(i)?;
            let c = s.chars().nth(i as usize)?;
            ok(JValue::Int(c as i32))
        }
        _ => None,
    }
}

fn boolean(b: bool, name: &str, args: &[JValue]) -> Invocation {
    match (name, args) {
        ("booleanValue", []) => ok(JValue::Bool(b)),
        ("toString", []) => ok(JValue::str(b.to_string())),
        ("compareTo", [JValue::Bool(o)]) => ok(JValue::Int(b as i32 - *o as i32)),
        _ => None,
    }
}

fn character(c: char, name: &str, args: &[JValue]) -> Invocation {
    match (name, args) {
        ("charValue", []) => ok(JValue::Char(c)),
        ("toString", []) => ok(JValue::str(c.to_string())),
        _ => None,
    }
}

/// A boxed number as `f64` (for `doubleValue()` and arithmetic).
pub fn as_f64(v: &JValue) -> Option<f64> {
    match v {
        JValue::Byte(b) => Some(*b as f64),
        JValue::Short(v) => Some(*v as f64),
        JValue::Int(i) => Some(*i as f64),
        JValue::Long(l) => Some(*l as f64),
        JValue::Float(f) => Some(*f as f64),
        JValue::Double(d) => Some(*d),
        JValue::BigInteger(s) | JValue::BigDecimal(s) => s.parse().ok(),
        _ => None,
    }
}

fn number(v: &JValue, name: &str, args: &[JValue]) -> Invocation {
    let d = as_f64(v)?;
    let long = match v {
        JValue::Byte(b) => *b as i64,
        JValue::Short(v) => *v as i64,
        JValue::Int(i) => *i as i64,
        JValue::Long(l) => *l,
        _ => d as i64,
    };
    match (name, args) {
        ("intValue", []) => ok(JValue::Int(long as i32)),
        ("longValue", []) => ok(JValue::Long(long)),
        ("shortValue", []) => ok(JValue::Int(long as i16 as i32)),
        ("byteValue", []) => ok(JValue::Int(long as i8 as i32)),
        ("doubleValue", []) => ok(JValue::Double(d)),
        ("floatValue", []) => ok(JValue::Float(d as f32)),
        ("isNaN", []) => ok(JValue::Bool(d.is_nan())),
        ("isInfinite", []) => ok(JValue::Bool(d.is_infinite())),
        ("compareTo", [o]) => {
            let o = as_f64(o)?;
            ok(JValue::Int(if d < o { -1 } else if d > o { 1 } else { 0 }))
        }
        _ => None,
    }
}

fn check_index(index: i64, len: usize) -> Result<usize, JError> {
    if index < 0 || index as usize >= len {
        Err(JError::new("java.lang.IndexOutOfBoundsException", format!("Index {index} out of bounds for length {len}")))
    } else {
        Ok(index as usize)
    }
}

/// `ArrayList.<method>`'s `Objects.checkIndex`.
fn array_list_index(index: i64, len: usize, method: &str) -> Result<usize, JError> {
    check_index(index, len).map_err(|e| {
        let frame = format!("java.base/java.util.ArrayList.{method}(Unknown Source)");
        e.with_stack(&[
            "java.base/jdk.internal.util.Preconditions.outOfBounds(Unknown Source)",
            "java.base/jdk.internal.util.Preconditions.outOfBoundsCheckIndex(Unknown Source)",
            "java.base/jdk.internal.util.Preconditions.checkIndex(Unknown Source)",
            "java.base/java.util.Objects.checkIndex(Unknown Source)",
            &frame,
        ])
    })
}

fn list(l: &JList, name: &str, args: &[JValue]) -> Invocation {
    match (name, args) {
        ("size", []) => ok(int(l.len())),
        ("isEmpty", []) => ok(JValue::Bool(l.is_empty())),
        ("get", [i]) if l.kind().is_array_list() || l.kind() == ListKind::Fixed => {
            let i = as_int(i)?;
            let index = if l.kind().is_array_list() { array_list_index(i, l.len(), "get") } else { check_index(i, l.len()) };
            Some(index.map(|i| l.get(i)))
        }
        ("getFirst", []) | ("getLast", []) if l.kind().is_array_list() || l.kind() == ListKind::Fixed || l.kind() == ListKind::Set => {
            let items = l.items();
            let v = if name == "getFirst" { items.first() } else { items.last() };
            match v {
                Some(v) => ok(v.clone()),
                None => err("java.util.NoSuchElementException", ""),
            }
        }
        ("contains", [v]) => ok(JValue::Bool(l.items().iter().any(|e| java_equals(e, v)))),
        ("containsAll", [JValue::List(other)]) => ok(JValue::Bool(other.items().iter().all(|v| l.items().iter().any(|e| java_equals(e, v))))),
        ("indexOf", [v]) => ok(JValue::Int(l.items().iter().position(|e| java_equals(e, v)).map(|i| i as i32).unwrap_or(-1))),
        ("lastIndexOf", [v]) => ok(JValue::Int(l.items().iter().rposition(|e| java_equals(e, v)).map(|i| i as i32).unwrap_or(-1))),
        ("add", [v]) => Some(l.modifiable().map(|_| {
            l.push(v.clone());
            Some(JValue::Bool(true))
        })),
        ("add", [i, v]) => {
            let i = as_int(i)?;
            Some(l.modifiable().and_then(|_| {
                let mut data = l.0.write();
                if i < 0 || i as usize > data.items.len() {
                    return Err(JError::new("java.lang.IndexOutOfBoundsException", format!("Index: {i}, Size: {}", data.items.len())));
                }
                data.items.insert(i as usize, v.clone());
                Ok(None)
            }))
        }
        ("addAll", [other]) => {
            let others = other.iterate()?;
            Some(l.modifiable().map(|_| {
                let changed = !others.is_empty();
                l.0.write().items.extend(others);
                Some(JValue::Bool(changed))
            }))
        }
        ("set", [i, v]) => {
            let i = as_int(i)?;
            Some(if l.kind() == ListKind::Set { Err(JError::without_message("java.lang.UnsupportedOperationException")) } else {
                let index = if l.kind().is_array_list() { array_list_index(i, l.len(), "set") } else { check_index(i, l.len()) };
                index.map(|i| Some(std::mem::replace(&mut l.0.write().items[i], v.clone())))
            })
        }
        ("remove", [JValue::Int(i)]) if l.kind().is_array_list() => {
            let i = *i as i64;
            Some(array_list_index(i, l.len(), "remove").map(|i| Some(l.0.write().items.remove(i))))
        }
        ("remove", [v]) => Some(l.modifiable().map(|_| {
            let mut data = l.0.write();
            let found = data.items.iter().position(|e| java_equals(e, v));
            if let Some(i) = found {
                data.items.remove(i);
            }
            Some(JValue::Bool(found.is_some()))
        })),
        ("clear", []) => Some(l.modifiable().map(|_| {
            l.0.write().items.clear();
            None
        })),
        ("subList", [a, b]) => {
            let (a, b) = (as_int(a)?, as_int(b)?);
            let items = l.items();
            if a < 0 || b as usize > items.len() || a > b {
                return err("java.lang.IndexOutOfBoundsException", format!("fromIndex = {a}, toIndex = {b}, size = {}", items.len()));
            }
            ok(JValue::List(JList::new(ListKind::Fixed, items[a as usize..b as usize].to_vec())))
        }
        ("toArray", []) => ok(JValue::Array(Arc::new(JArray::new(JType::object(), l.items())))),
        ("iterator", []) => ok(JValue::List(JList::new(ListKind::Iterator, l.items()))),
        _ => None,
    }
}

fn map(m: &JMap, name: &str, args: &[JValue]) -> Invocation {
    let unsupported = || Some(Err(JError::without_message("java.lang.UnsupportedOperationException")));
    match (name, args) {
        ("size", []) => ok(int(m.len())),
        ("isEmpty", []) => ok(JValue::Bool(m.is_empty())),
        ("get", [k]) => ok(m.get(k).unwrap_or(JValue::Null)),
        ("getOrDefault", [k, d]) => ok(m.get(k).unwrap_or_else(|| d.clone())),
        ("containsKey", [k]) => ok(JValue::Bool(m.contains_key(k))),
        ("containsValue", [v]) => ok(JValue::Bool(m.entries().iter().any(|(_, e)| java_equals(e, v)))),
        ("put", [k, v]) if m.kind() != MapKind::Fixed => ok(m.put(k.clone(), v.clone())),
        ("putIfAbsent", [k, v]) if m.kind() != MapKind::Fixed => match m.get(k) {
            Some(existing) if !existing.is_null() => ok(existing),
            _ => ok(m.put(k.clone(), v.clone())),
        },
        ("putAll", [JValue::Map(other)]) if m.kind() != MapKind::Fixed => {
            for (k, v) in other.entries() {
                m.put(k, v);
            }
            Some(Ok(None))
        }
        ("remove", [k]) if m.kind() != MapKind::Fixed => ok(m.remove(k)),
        ("clear", []) if m.kind() != MapKind::Fixed => {
            for (k, _) in m.entries() {
                m.remove(&k);
            }
            Some(Ok(None))
        }
        ("put" | "putIfAbsent" | "putAll" | "remove" | "clear", _) => unsupported(),
        ("keySet", []) => ok(JValue::List(JList::new(ListKind::Set, m.entries().into_iter().map(|(k, _)| k).collect()))),
        ("values", []) => ok(JValue::List(JList::new(ListKind::Collection, m.entries().into_iter().map(|(_, v)| v).collect()))),
        ("entrySet", []) => ok(JValue::List(JList::new(
            ListKind::Set,
            m.entries().into_iter().map(|(key, value)| JValue::Object(Arc::new(MapEntry { key, value }) as Arc<dyn JObject>)).collect(),
        ))),
        _ => None,
    }
}

fn array(a: &Arc<JArray>, name: &str, args: &[JValue]) -> Invocation {
    // Arrays have no methods of their own; `length` is a field, and `clone()` is public.
    match (name, args) {
        ("clone", []) => ok(JValue::Array(Arc::new(JArray::new(a.component.clone(), a.items())))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(target: JValue, name: &str, args: &[JValue]) -> JValue {
        invoke(&target, name, args).expect("method exists").expect("no exception").expect("not void")
    }

    #[test]
    fn string_split_follows_java() {
        let parts = |s: &str, p: &str, limit: i64| split(s, p, limit).unwrap();
        assert_eq!(parts("a,b,,c,,", ",", 0), vec!["a", "b", "", "c"]);
        assert_eq!(parts("a,b,,c,,", ",", -1), vec!["a", "b", "", "c", "", ""]);
        assert_eq!(parts("a,b,c", ",", 2), vec!["a", "b,c"]);
        assert_eq!(parts(",a", ",", 0), vec!["", "a"]);
        assert_eq!(parts(",", ",", 0), Vec::<String>::new());
        assert_eq!(parts("abc", "", 0), vec!["a", "b", "c"]);
        assert_eq!(parts("", ",", 0), vec![""]);
    }

    #[test]
    fn string_methods() {
        let s = JValue::str("Hello World");
        assert_eq!(call(s.clone(), "length", &[]).to_java_string(), "11");
        assert_eq!(call(s.clone(), "substring", &[JValue::Int(6)]).to_java_string(), "World");
        assert_eq!(call(s.clone(), "indexOf", &[JValue::str("o")]).to_java_string(), "4");
        assert_eq!(call(s.clone(), "lastIndexOf", &[JValue::str("o")]).to_java_string(), "7");
        assert_eq!(call(s.clone(), "replaceAll", &[JValue::str("(\\w+) (\\w+)"), JValue::str("$2 $1")]).to_java_string(), "World Hello");
        assert_eq!(call(s.clone(), "toUpperCase", &[]).to_java_string(), "HELLO WORLD");
        assert!(invoke(&s, "substring", &[JValue::Int(20)]).unwrap().is_err());
        assert!(invoke(&s, "nope", &[]).is_none());
    }
}
