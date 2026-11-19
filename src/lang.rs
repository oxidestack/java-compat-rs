//! A small model of Java values for template engines that walk Java objects reflectively (JMustache,
//! Velocity): strings, boxed numbers, collections and maps with their `java.util` semantics (`toString`,
//! `hashCode`, `HashMap` iteration order), and arbitrary objects through [`JObject`].

use std::fmt;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::number::{double_to_string, float_to_string};
use crate::reflect::JType;

/// A Java value. Collections and maps are shared and mutable, as Java references are.
#[derive(Clone)]
pub enum JValue {
    /// `null`.
    Null,
    /// `boolean` / `Boolean`.
    Bool(bool),
    /// `char` / `Character`.
    Char(char),
    /// `byte` / `Byte`.
    Byte(i8),
    /// `short` / `Short`.
    Short(i16),
    /// `int` / `Integer`.
    Int(i32),
    /// `long` / `Long`.
    Long(i64),
    /// `float` / `Float`.
    Float(f32),
    /// `double` / `Double`.
    Double(f64),
    /// `java.math.BigInteger` / `BigDecimal`, by their `toString()`.
    BigInteger(Arc<str>),
    /// `java.math.BigDecimal`, by its `toString()`.
    BigDecimal(Arc<str>),
    /// `java.lang.String`.
    Str(Arc<str>),
    /// A `java.util.List` (or another collection, by its [`ListKind`]).
    List(JList),
    /// A `java.util.Map`.
    Map(JMap),
    /// A Java array (`String[]`, `byte[]`, `Object[]`, …).
    Array(Arc<JArray>),
    /// Any other object.
    Object(Arc<dyn JObject>),
}

/// A Java object other than the built-in value types.
pub trait JObject: Send + Sync {
    /// The binary class name (`org.mockserver.templates.engine.model.HttpRequestTemplateObject`).
    fn class_name(&self) -> String;

    /// Invokes a public method. `None` when the class (or a superclass other than `Object`) declares no such
    /// method for these arguments; `Ok(None)` for a `void` method.
    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>>;

    /// Reads a field by name (JMustache falls back to fields); `None` when there is no such field.
    fn field(&self, _name: &str) -> Option<JValue> {
        None
    }

    /// `toString()`.
    fn to_java_string(&self) -> String;

    /// The elements, when the object is `Iterable`.
    fn iterate(&self) -> Option<Vec<JValue>> {
        None
    }

    /// Engine-specific capabilities (a Mustache lambda, a Velocity tool), for downcasting.
    fn as_any(&self) -> &dyn std::any::Any;

    /// What reflection sees of the class, when it matters which members are declared where (JMustache's
    /// `DefaultCollector` searches declared members class by class). `None`: the object only answers `invoke`
    /// and `field`, as if every member were public and declared on its own class.
    fn reflection(&self) -> Option<Reflection> {
        None
    }

    /// The typed public API (`getMethods()`, supertypes), for engines that resolve overloads like Java
    /// reflection users do. `None`: only [`JObject::invoke`] is known.
    fn class_def(&self) -> Option<Arc<crate::reflect::ClassDef>> {
        None
    }
}

/// A method's return type, as far as member lookup cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Returns {
    /// `void`.
    Void,
    /// `boolean` or `Boolean`.
    Boolean,
    /// Any other type.
    Other,
}

#[derive(Debug, Clone, PartialEq)]
/// A zero-argument method a class declares.
pub struct MethodDecl {
    /// The method's name.
    pub name: String,
    /// What it returns.
    pub returns: Returns,
    /// Whether it is `public`.
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq)]
/// A field a class declares.
pub struct FieldDecl {
    /// The field's name.
    pub name: String,
    /// Whether it is `public`.
    pub public: bool,
}

/// The members one class or interface declares (`getDeclaredMethods` / `getDeclaredFields`), zero-argument
/// methods only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Members {
    /// `getDeclaredMethods()`, zero-argument ones.
    pub methods: Vec<MethodDecl>,
    /// `getDeclaredFields()`.
    pub fields: Vec<FieldDecl>,
}

impl Members {
    /// Adds a declared method.
    pub fn method(mut self, name: &str, returns: Returns, public: bool) -> Members {
        self.methods.push(MethodDecl { name: name.into(), returns, public });
        self
    }

    /// Adds a declared field.
    pub fn field(mut self, name: &str, public: bool) -> Members {
        self.fields.push(FieldDecl { name: name.into(), public });
        self
    }
}

/// A class as reflection sees it: its own members, then each superclass's up to (not including) `Object`,
/// and the interfaces in the order `getInterfaces()` walks them (each followed by its super-interfaces).
/// Invocation still goes through [`JObject::invoke`] / [`JObject::field`], which dispatch like Java.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reflection {
    /// The class's own members, then each superclass's.
    pub classes: Vec<Members>,
    /// The interfaces' members, in `getInterfaces()` order.
    pub interfaces: Vec<Members>,
}

/// A Java exception, by class and message, with the JDK frames it was thrown from where they are known (as
/// `printStackTrace` prints them on MockServer's trimmed JDK: `java.base/java.util.ArrayList.get(Unknown Source)`).
#[derive(Debug, Clone, PartialEq)]
pub struct JError {
    /// The exception's class name (`java.lang.IllegalArgumentException`).
    pub class: String,
    /// `getMessage()`.
    pub message: Option<String>,
    /// The frames it was thrown from, innermost first.
    pub stack_trace: Vec<String>,
}

impl JError {
    /// An exception of `class` with this message.
    pub fn new(class: &str, message: impl Into<String>) -> JError {
        JError { class: class.into(), message: Some(message.into()), stack_trace: Vec::new() }
    }

    /// The frames it was thrown from, innermost first (`StackTraceElement.toString()`).
    pub fn with_stack(mut self, frames: &[&str]) -> JError {
        self.stack_trace = frames.iter().map(|f| f.to_string()).collect();
        self
    }

    /// `printStackTrace()`: `toString()`, then a tab-indented `at` line per known frame.
    pub fn print_stack_trace(&self) -> String {
        let mut out = format!("{self}\n");
        for f in &self.stack_trace {
            out.push_str(&format!("\tat {f}\n"));
        }
        out
    }

    /// An exception of `class` whose `getMessage()` is null.
    pub fn without_message(class: &str) -> JError {
        JError { class: class.into(), message: None, stack_trace: Vec::new() }
    }
}

impl fmt::Display for JError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.message {
            Some(m) => write!(f, "{}: {m}", self.class),
            None => f.write_str(&self.class),
        }
    }
}

/// How a list was created, which decides its class name and whether it can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// `java.util.ArrayList`.
    ArrayList,
    /// `Arrays.asList`, `List.of`, unmodifiable views.
    Fixed,
    /// A `Set` or a map's `keySet()` / `entrySet()`.
    Set,
    /// A map's `values()`.
    Collection,
    /// An `Iterator` (`iterator()`).
    Iterator,
    /// json-smart's `net.minidev.json.JSONArray` (an `ArrayList` whose `toString()` is compact JSON), as Jayway
    /// JsonPath returns arrays.
    JsonArray,
}

impl ListKind {
    /// An `ArrayList` or a subclass of it.
    pub fn is_array_list(self) -> bool {
        matches!(self, ListKind::ArrayList | ListKind::JsonArray)
    }
}

#[derive(Clone)]
/// A Java list, shared: clones are the same list, as Java references are.
pub struct JList(pub(crate) Arc<RwLock<ListData>>);

pub(crate) struct ListData {
    pub(crate) kind: ListKind,
    pub(crate) items: Vec<JValue>,
}

impl JList {
    /// A list of this kind holding `items`.
    pub fn new(kind: ListKind, items: Vec<JValue>) -> JList {
        JList(Arc::new(RwLock::new(ListData { kind, items })))
    }

    /// A `java.util.ArrayList` holding `items`.
    pub fn array_list(items: Vec<JValue>) -> JList {
        JList::new(ListKind::ArrayList, items)
    }

    /// How the list was created.
    pub fn kind(&self) -> ListKind {
        self.0.read().kind
    }

    /// A copy of the elements.
    pub fn items(&self) -> Vec<JValue> {
        self.0.read().items.clone()
    }

    /// `size()`.
    pub fn len(&self) -> usize {
        self.0.read().items.len()
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `get(index)`, `None` past the end.
    pub fn get(&self, index: usize) -> Option<JValue> {
        self.0.read().items.get(index).cloned()
    }

    /// `add(v)`.
    pub fn push(&self, v: JValue) {
        self.0.write().items.push(v);
    }

    /// Whether both are the same list (`==`).
    pub fn same(&self, other: &JList) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn modifiable(&self) -> Result<(), JError> {
        match self.kind() {
            ListKind::ArrayList | ListKind::JsonArray => Ok(()),
            _ => Err(JError::without_message("java.lang.UnsupportedOperationException")),
        }
    }
}

/// `java.util` map flavours, which differ in iteration order and key comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// `HashMap` / `ConcurrentHashMap`: iteration by hash bucket.
    Hash,
    /// `LinkedHashMap`: insertion order.
    Linked,
    /// A `LinkedHashMap` whose `String` keys compare case-insensitively (MockServer's header map).
    CaseInsensitive,
    /// Immutable (`Map.of`, `Collections.unmodifiableMap`), insertion order.
    Fixed,
}

#[derive(Clone)]
/// A Java map, shared: clones are the same map, as Java references are.
pub struct JMap(Arc<RwLock<MapData>>);

struct MapData {
    kind: MapKind,
    entries: Vec<(JValue, JValue)>,
    /// `HashMap` table size, which only grows.
    capacity: usize,
}

impl JMap {
    /// An empty map of this kind.
    pub fn new(kind: MapKind) -> JMap {
        JMap(Arc::new(RwLock::new(MapData { kind, entries: Vec::new(), capacity: 16 })))
    }

    /// A map of this kind holding `entries`, put in order.
    pub fn from_entries(kind: MapKind, entries: impl IntoIterator<Item = (JValue, JValue)>) -> JMap {
        let map = JMap::new(kind);
        for (k, v) in entries {
            map.0.write().put(k, v);
        }
        map
    }

    /// Which `Map` class it is.
    pub fn kind(&self) -> MapKind {
        self.0.read().kind
    }

    /// Entries in Java iteration order.
    pub fn entries(&self) -> Vec<(JValue, JValue)> {
        self.0.read().ordered()
    }

    /// `size()`.
    pub fn len(&self) -> usize {
        self.0.read().entries.len()
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `get(key)`, `None` when absent.
    pub fn get(&self, key: &JValue) -> Option<JValue> {
        let data = self.0.read();
        data.find(key).map(|i| data.entries[i].1.clone())
    }

    /// `get(key)` for a string key.
    pub fn get_str(&self, key: &str) -> Option<JValue> {
        self.get(&JValue::str(key))
    }

    /// `containsKey(key)`.
    pub fn contains_key(&self, key: &JValue) -> bool {
        self.0.read().find(key).is_some()
    }

    /// `put`: the previous value, or `Null`.
    pub fn put(&self, key: JValue, value: JValue) -> JValue {
        self.0.write().put(key, value)
    }

    /// `put(key, value)` for a string key.
    pub fn put_str(&self, key: &str, value: JValue) {
        self.put(JValue::str(key), value);
    }

    /// `remove(key)`: the value removed, null when absent.
    pub fn remove(&self, key: &JValue) -> JValue {
        let mut data = self.0.write();
        match data.find(key) {
            Some(i) => data.entries.remove(i).1,
            None => JValue::Null,
        }
    }

    /// Whether both are the same map (`==`).
    pub fn same(&self, other: &JMap) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl MapData {
    fn find(&self, key: &JValue) -> Option<usize> {
        match self.kind {
            MapKind::CaseInsensitive => {
                let wanted = key.as_str().map(str::to_lowercase);
                self.entries.iter().position(|(k, _)| match (&wanted, k.as_str()) {
                    (Some(w), Some(k)) => *w == k.to_lowercase(),
                    _ => java_equals(k, key),
                })
            }
            _ => self.entries.iter().position(|(k, _)| java_equals(k, key)),
        }
    }

    fn put(&mut self, key: JValue, value: JValue) -> JValue {
        if let Some(i) = self.find(&key) {
            return std::mem::replace(&mut self.entries[i].1, value);
        }
        self.entries.push((key, value));
        while self.entries.len() as f64 > self.capacity as f64 * 0.75 {
            self.capacity *= 2;
        }
        JValue::Null
    }

    fn ordered(&self) -> Vec<(JValue, JValue)> {
        match self.kind {
            MapKind::Hash => {
                let mut indexed: Vec<(usize, usize)> = self
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, (k, _))| {
                        let h = java_hash_code(k);
                        let spread = h ^ ((h as u32) >> 16) as i32;
                        ((spread as u32 as usize) & (self.capacity - 1), i)
                    })
                    .collect();
                indexed.sort();
                indexed.into_iter().map(|(_, i)| self.entries[i].clone()).collect()
            }
            _ => self.entries.clone(),
        }
    }
}

/// A Java array: its component type (which decides its class name) and elements.
pub struct JArray {
    /// The component type (`String` for `String[]`).
    pub component: JType,
    items: RwLock<Vec<JValue>>,
}

impl JArray {
    /// An array of `component` holding `items`.
    pub fn new(component: JType, items: Vec<JValue>) -> JArray {
        JArray { component, items: RwLock::new(items) }
    }

    /// A copy of the elements.
    pub fn items(&self) -> Vec<JValue> {
        self.items.read().clone()
    }

    /// `length`.
    pub fn len(&self) -> usize {
        self.items.read().len()
    }

    /// Whether `length` is 0.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The element at `index`, `None` past the end.
    pub fn get(&self, index: usize) -> Option<JValue> {
        self.items.read().get(index).cloned()
    }

    /// `Array.set`: `false` when out of bounds.
    pub fn set(&self, index: usize, v: JValue) -> bool {
        match self.items.write().get_mut(index) {
            Some(slot) => {
                *slot = v;
                true
            }
            None => false,
        }
    }

    /// An `Object[]`.
    pub fn objects(items: Vec<JValue>) -> JValue {
        JValue::Array(Arc::new(JArray::new(JType::object(), items)))
    }

    /// A `String[]`.
    pub fn strings(items: impl IntoIterator<Item = impl Into<Arc<str>>>) -> JValue {
        JValue::Array(Arc::new(JArray::new(JType::string(), items.into_iter().map(|s| JValue::Str(s.into())).collect())))
    }
}

impl JValue {
    /// A `java.lang.String`.
    pub fn str(s: impl Into<Arc<str>>) -> JValue {
        JValue::Str(s.into())
    }

    /// The text of a `String`, `None` for any other value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Whether it is `null`.
    pub fn is_null(&self) -> bool {
        matches!(self, JValue::Null)
    }

    /// An `ArrayList` of strings.
    pub fn string_list(items: impl IntoIterator<Item = impl Into<Arc<str>>>) -> JValue {
        JValue::List(JList::array_list(items.into_iter().map(|s| JValue::Str(s.into())).collect()))
    }

    /// `getClass().getName()`.
    pub fn class_name(&self) -> String {
        match self {
            JValue::Null => "null".into(),
            JValue::Bool(_) => "java.lang.Boolean".into(),
            JValue::Char(_) => "java.lang.Character".into(),
            JValue::Byte(_) => "java.lang.Byte".into(),
            JValue::Short(_) => "java.lang.Short".into(),
            JValue::Int(_) => "java.lang.Integer".into(),
            JValue::Long(_) => "java.lang.Long".into(),
            JValue::Float(_) => "java.lang.Float".into(),
            JValue::Double(_) => "java.lang.Double".into(),
            JValue::BigInteger(_) => "java.math.BigInteger".into(),
            JValue::BigDecimal(_) => "java.math.BigDecimal".into(),
            JValue::Str(_) => "java.lang.String".into(),
            JValue::List(l) => match l.kind() {
                ListKind::ArrayList => "java.util.ArrayList".into(),
                ListKind::Fixed => "java.util.Arrays$ArrayList".into(),
                ListKind::Set => "java.util.LinkedHashSet".into(),
                ListKind::Collection => "java.util.LinkedHashMap$LinkedValues".into(),
                ListKind::Iterator => "java.util.ArrayList$Itr".into(),
                ListKind::JsonArray => "net.minidev.json.JSONArray".into(),
            },
            JValue::Map(m) => match m.kind() {
                MapKind::Hash => "java.util.HashMap".into(),
                MapKind::Linked | MapKind::Fixed => "java.util.LinkedHashMap".into(),
                MapKind::CaseInsensitive => "org.mockserver.templates.engine.model.CaseInsensitiveHeaderMap".into(),
            },
            JValue::Array(a) => JType::array(a.component.clone()).name(),
            JValue::Object(o) => o.class_name(),
        }
    }

    /// `String.valueOf(value)`.
    pub fn to_java_string(&self) -> String {
        self.render(&mut Vec::new())
    }

    fn render(&self, seen: &mut Vec<usize>) -> String {
        match self {
            JValue::Null => "null".into(),
            JValue::Bool(b) => b.to_string(),
            JValue::Char(c) => c.to_string(),
            JValue::Byte(b) => b.to_string(),
            JValue::Short(v) => v.to_string(),
            JValue::Int(i) => i.to_string(),
            JValue::Long(l) => l.to_string(),
            JValue::Float(f) => float_to_string(*f),
            JValue::Double(d) => double_to_string(*d),
            JValue::BigInteger(s) | JValue::BigDecimal(s) | JValue::Str(s) => s.to_string(),
            JValue::List(l) if l.kind() == ListKind::JsonArray => json_smart(self),
            JValue::List(l) => {
                let id = Arc::as_ptr(&l.0) as usize;
                seen.push(id);
                let items: Vec<String> = l
                    .items()
                    .iter()
                    .map(|v| match v {
                        JValue::List(inner) if seen.contains(&(Arc::as_ptr(&inner.0) as usize)) => "(this Collection)".into(),
                        other => other.render(seen),
                    })
                    .collect();
                seen.pop();
                format!("[{}]", items.join(", "))
            }
            JValue::Map(m) => {
                let id = Arc::as_ptr(&m.0) as usize;
                seen.push(id);
                let render = |v: &JValue, seen: &mut Vec<usize>| match v {
                    JValue::Map(inner) if seen.contains(&(Arc::as_ptr(&inner.0) as usize)) => "(this Map)".into(),
                    other => other.render(seen),
                };
                let items: Vec<String> = m.entries().iter().map(|(k, v)| format!("{}={}", render(k, seen), render(v, seen))).collect();
                seen.pop();
                format!("{{{}}}", items.join(", "))
            }
            JValue::Array(a) => {
                // `Object.toString()`: class name and identity hash, which is not reproducible.
                let hash = Arc::as_ptr(a) as usize as u32;
                format!("{}@{hash:x}", self.class_name())
            }
            JValue::Object(o) => o.to_java_string(),
        }
    }

    /// Elements when the value is `Iterable` (or an array).
    pub fn iterate(&self) -> Option<Vec<JValue>> {
        match self {
            JValue::List(l) => Some(l.items()),
            JValue::Array(a) => Some(a.items()),
            JValue::Object(o) => o.iterate(),
            _ => None,
        }
    }
}

impl fmt::Debug for JValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.class_name(), self.to_java_string())
    }
}

/// json-smart's `JSONValue.toJSONString(value)` with its default `JSONStyle.NO_COMPRESS`: keys and strings
/// always quoted, `/` and web-unsafe characters escaped.
pub fn json_smart(v: &JValue) -> String {
    fn quote(s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '/' => out.push_str("\\/"),
                '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2000}'..='\u{20ff}' => out.push_str(&format!("\\u{:04X}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    fn write(v: &JValue, out: &mut String) {
        match v {
            JValue::Null => out.push_str("null"),
            JValue::Bool(_) | JValue::Int(_) | JValue::Long(_) | JValue::Float(_) | JValue::Double(_) | JValue::BigInteger(_) | JValue::BigDecimal(_) => {
                out.push_str(&v.to_java_string())
            }
            JValue::List(l) => {
                out.push('[');
                for (i, item) in l.items().iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(']');
            }
            JValue::Map(m) => {
                out.push('{');
                for (i, (k, item)) in m.entries().iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    quote(&k.to_java_string(), out);
                    out.push(':');
                    write(item, out);
                }
                out.push('}');
            }
            other => quote(&other.to_java_string(), out),
        }
    }
    let mut out = String::new();
    write(v, &mut out);
    out
}

/// `Objects.equals(a, b)` for the built-in types (objects compare by identity).
pub fn java_equals(a: &JValue, b: &JValue) -> bool {
    match (a, b) {
        (JValue::Null, JValue::Null) => true,
        (JValue::Bool(x), JValue::Bool(y)) => x == y,
        (JValue::Char(x), JValue::Char(y)) => x == y,
        (JValue::Byte(x), JValue::Byte(y)) => x == y,
        (JValue::Short(x), JValue::Short(y)) => x == y,
        (JValue::Int(x), JValue::Int(y)) => x == y,
        (JValue::Long(x), JValue::Long(y)) => x == y,
        (JValue::Float(x), JValue::Float(y)) => x.to_bits() == y.to_bits(),
        (JValue::Double(x), JValue::Double(y)) => x.to_bits() == y.to_bits(),
        (JValue::BigInteger(x), JValue::BigInteger(y)) | (JValue::BigDecimal(x), JValue::BigDecimal(y)) | (JValue::Str(x), JValue::Str(y)) => x == y,
        (JValue::List(x), JValue::List(y)) => {
            x.same(y) || {
                let (xs, ys) = (x.items(), y.items());
                xs.len() == ys.len() && xs.iter().zip(&ys).all(|(a, b)| java_equals(a, b))
            }
        }
        (JValue::Map(x), JValue::Map(y)) => {
            x.same(y) || (x.len() == y.len() && x.entries().iter().all(|(k, v)| y.get(k).is_some_and(|w| java_equals(v, &w))))
        }
        (JValue::Array(x), JValue::Array(y)) => Arc::ptr_eq(x, y),
        (JValue::Object(x), JValue::Object(y)) => std::ptr::addr_eq(Arc::as_ptr(x), Arc::as_ptr(y)),
        _ => false,
    }
}

/// `Objects.hashCode(value)`.
pub fn java_hash_code(v: &JValue) -> i32 {
    match v {
        JValue::Null => 0,
        JValue::Byte(b) => *b as i32,
        JValue::Short(s) => *s as i32,
        JValue::Bool(b) => {
            if *b {
                1231
            } else {
                1237
            }
        }
        JValue::Char(c) => *c as i32,
        JValue::Int(i) => *i,
        JValue::Long(l) => (*l ^ ((*l as u64) >> 32) as i64) as i32,
        JValue::Float(f) => f.to_bits() as i32,
        JValue::Double(d) => {
            let bits = if d.is_nan() { 0x7ff8000000000000u64 } else { d.to_bits() };
            (bits ^ (bits >> 32)) as i32
        }
        JValue::Str(s) => string_hash(s),
        JValue::BigInteger(s) => big_integer_hash(s),
        JValue::BigDecimal(s) => big_decimal_hash(s),
        JValue::List(l) => l.items().iter().fold(1i32, |h, e| h.wrapping_mul(31).wrapping_add(java_hash_code(e))),
        JValue::Map(m) => m.entries().iter().fold(0i32, |h, (k, v)| h.wrapping_add(java_hash_code(k) ^ java_hash_code(v))),
        JValue::Array(a) => Arc::as_ptr(a) as usize as i32,
        JValue::Object(o) => match o.invoke("hashCode", &[]) {
            Some(Ok(Some(JValue::Int(h)))) => h,
            _ => Arc::as_ptr(o) as *const () as usize as i32,
        },
    }
}

/// `BigInteger.hashCode()` of a decimal literal: the magnitude's 32-bit words, most significant first.
pub fn big_integer_hash(s: &str) -> i32 {
    let Ok(b) = s.parse::<num_bigint::BigInt>() else { return string_hash(s) };
    big_int_hash(&b)
}

fn big_int_hash(b: &num_bigint::BigInt) -> i32 {
    let (sign, words) = b.to_u32_digits();
    let h = words.iter().rev().fold(0i32, |h, w| h.wrapping_mul(31).wrapping_add(*w as i32));
    match sign {
        num_bigint::Sign::Minus => h.wrapping_neg(),
        num_bigint::Sign::NoSign => 0,
        num_bigint::Sign::Plus => h,
    }
}

/// `BigDecimal.hashCode()` of its `toString()`: `31 * unscaledValue().hashCode() + scale`.
pub fn big_decimal_hash(s: &str) -> i32 {
    let Some(d) = crate::text::big_decimal_parts(s) else { return string_hash(s) };
    big_int_hash(&d.0).wrapping_mul(31).wrapping_add(d.1 as i32)
}

/// `String.hashCode()`.
pub fn string_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(u as i32))
}

/// A `Map.Entry` as a map's `entrySet()` yields it.
pub struct MapEntry {
    /// `getKey()`.
    pub key: JValue,
    /// `getValue()`.
    pub value: JValue,
}

impl JObject for MapEntry {
    fn class_name(&self) -> String {
        "java.util.LinkedHashMap$Entry".into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        match (name, args) {
            ("getKey", []) => Some(Ok(Some(self.key.clone()))),
            ("getValue", []) => Some(Ok(Some(self.value.clone()))),
            ("toString", []) => Some(Ok(Some(JValue::str(self.to_java_string())))),
            ("hashCode", []) => Some(Ok(Some(JValue::Int(java_hash_code(&self.key) ^ java_hash_code(&self.value))))),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        format!("{}={}", self.key.to_java_string(), self.value.to_java_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_map_iterates_by_bucket() {
        // `new HashMap<>()` with a, b, content-type, host: Java prints {a=1, b=2, host=4, content-type=3}.
        let m = JMap::new(MapKind::Hash);
        for (k, v) in [("b", 2), ("a", 1), ("content-type", 3), ("host", 4)] {
            m.put_str(k, JValue::Int(v));
        }
        assert_eq!(JValue::Map(m).to_java_string(), "{a=1, b=2, host=4, content-type=3}");
    }

    #[test]
    fn json_array_prints_as_json_smart() {
        // Jayway `$.a` on {"a": [1, 2.50, "x/y", {"k": "v\"q"}, [true, null]]}: JSONArray.toString().
        let inner = JMap::new(MapKind::Linked);
        inner.put_str("k", JValue::str("v\"q"));
        let nested = JList::new(ListKind::JsonArray, vec![JValue::Bool(true), JValue::Null]);
        let a = JList::new(ListKind::JsonArray, vec![JValue::Int(1), JValue::Double(2.5), JValue::str("x/y"), JValue::Map(inner.clone()), JValue::List(nested)]);
        assert_eq!(JValue::List(a).to_java_string(), r#"[1,2.5,"x\/y",{"k":"v\"q"},[true,null]]"#);
        // A map holding one keeps `AbstractMap.toString()`.
        inner.put_str("l", JValue::List(JList::new(ListKind::JsonArray, vec![JValue::str("\u{1}")])));
        assert_eq!(JValue::Map(inner).to_java_string(), r#"{k=v"q, l=["\u0001"]}"#);
    }

    #[test]
    fn collections_print_like_java() {
        let l = JList::array_list(vec![JValue::str("x"), JValue::Int(1), JValue::Double(2.0), JValue::Null]);
        assert_eq!(JValue::List(l.clone()).to_java_string(), "[x, 1, 2.0, null]");
        l.push(JValue::List(l.clone()));
        assert_eq!(JValue::List(l).to_java_string(), "[x, 1, 2.0, null, (this Collection)]");
        assert_eq!(string_hash("host"), 3208616);
    }
}

/// A decimal string (`BigInteger`/`BigDecimal.toString()`) as sign, significant digits and the power of ten of
/// the digit string's end, for exact comparison.
fn decimal_parts(s: &str) -> (bool, String, i64) {
    let (negative, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (mantissa, exp) = match rest.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (rest, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{int}{frac}");
    let mut exp = exp - frac.len() as i64;
    let digits = digits.trim_start_matches('0');
    let trimmed = digits.trim_end_matches('0');
    exp += (digits.len() - trimmed.len()) as i64;
    (negative && !trimmed.is_empty(), trimmed.to_string(), exp)
}

/// Exact comparison of two decimal strings.
pub fn compare_decimal_strings(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (an, ad, ae) = decimal_parts(a);
    let (bn, bd, be) = decimal_parts(b);
    let magnitude = || {
        match (ad.is_empty(), bd.is_empty()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        // Compare by the position of the most significant digit, then digit by digit.
        let (ta, tb) = (ad.len() as i64 + ae, bd.len() as i64 + be);
        if ta != tb {
            return ta.cmp(&tb);
        }
        let n = ad.len().max(bd.len());
        let pad = |d: &str| format!("{d:0<n$}");
        pad(&ad).cmp(&pad(&bd))
    };
    match (an, bn) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        (false, false) => magnitude(),
        (true, true) => magnitude().reverse(),
    }
}

/// `Character.toUpperCase(char)` / `toLowerCase(char)`: simple (one-to-one) case mappings.
pub fn char_upper(c: char) -> char {
    let mut u = c.to_uppercase();
    match (u.next(), u.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

/// `Character.toLowerCase(c)`.
pub fn char_lower(c: char) -> char {
    let mut l = c.to_lowercase();
    match (l.next(), l.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

/// `String.compareToIgnoreCase`.
/// `String.trim()`: strips every leading and trailing character up to U+0020.
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `StringUtils.trim` (commons-lang3): `null`-safe [`java_trim`].
pub fn string_utils_trim(s: Option<&str>) -> Option<&str> {
    s.map(java_trim)
}

/// `String.compareToIgnoreCase`: negative, zero or positive.
pub fn compare_ignore_case(a: &str, b: &str) -> i32 {
    let (x, y): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    for (c1, c2) in x.iter().zip(&y) {
        if c1 != c2 {
            let (u1, u2) = (char_upper(*c1), char_upper(*c2));
            if u1 != u2 {
                let (l1, l2) = (char_lower(u1), char_lower(u2));
                if l1 != l2 {
                    return l1 as i32 - l2 as i32;
                }
            }
        }
    }
    x.len() as i32 - y.len() as i32
}

/// `((Comparable) a).compareTo(b)`: `Err` with `ClassCastException` when the classes differ (or `a` is not
/// `Comparable`), `NullPointerException` for a `null` argument.
pub fn compare_to(a: &JValue, b: &JValue) -> Result<i32, JError> {
    use std::cmp::Ordering;
    let ord = |o: Ordering| o as i32;
    let cast = || {
        JError::new(
            "java.lang.ClassCastException",
            format!("class {} cannot be cast to class {}", b.class_name(), a.class_name()),
        )
    };
    Ok(match (a, b) {
        (JValue::Null, _) | (_, JValue::Null) => return Err(JError::without_message("java.lang.NullPointerException")),
        (JValue::Str(x), JValue::Str(y)) => {
            let (x, y): (Vec<u16>, Vec<u16>) = (x.encode_utf16().collect(), y.encode_utf16().collect());
            x.iter().zip(&y).find(|(p, q)| p != q).map(|(p, q)| *p as i32 - *q as i32).unwrap_or(x.len() as i32 - y.len() as i32)
        }
        (JValue::Byte(x), JValue::Byte(y)) => *x as i32 - *y as i32,
        (JValue::Short(x), JValue::Short(y)) => *x as i32 - *y as i32,
        (JValue::Int(x), JValue::Int(y)) => ord(x.cmp(y)),
        (JValue::Long(x), JValue::Long(y)) => ord(x.cmp(y)),
        (JValue::Double(x), JValue::Double(y)) => ord(java_double_compare(*x, *y)),
        (JValue::Float(x), JValue::Float(y)) => ord(java_double_compare(*x as f64, *y as f64)),
        (JValue::Bool(x), JValue::Bool(y)) => ord(x.cmp(y)),
        (JValue::Char(x), JValue::Char(y)) => *x as i32 - *y as i32,
        (JValue::BigInteger(x), JValue::BigInteger(y)) | (JValue::BigDecimal(x), JValue::BigDecimal(y)) => ord(compare_decimal_strings(x, y)),
        (JValue::Object(o), _) => match o.invoke("compareTo", std::slice::from_ref(b)) {
            Some(Ok(Some(JValue::Int(i)))) => i,
            Some(Err(e)) => return Err(e),
            _ => return Err(cast()),
        },
        _ => return Err(cast()),
    })
}

/// `Double.compare`: `-0.0 < 0.0`, `NaN` above everything and equal to itself.
pub fn java_double_compare(x: f64, y: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if x < y {
        return Ordering::Less;
    }
    if x > y {
        return Ordering::Greater;
    }
    let bits = |d: f64| if d.is_nan() { 0x7ff8_0000_0000_0000i64 } else { d.to_bits() as i64 };
    bits(x).cmp(&bits(y))
}

#[cfg(test)]
mod compare_tests {
    use super::*;

    #[test]
    fn comparable_semantics() {
        assert!(compare_to(&JValue::str("a"), &JValue::str("b")).unwrap() < 0);
        assert!(compare_to(&JValue::Int(1), &JValue::Long(1)).is_err());
        assert_eq!(compare_to(&JValue::BigDecimal("2.0".into()), &JValue::BigDecimal("2.00".into())).unwrap(), 0);
        assert!(compare_to(&JValue::BigDecimal("-10".into()), &JValue::BigDecimal("-9.5".into())).unwrap() < 0);
        assert!(compare_to(&JValue::Double(-0.0), &JValue::Double(0.0)).unwrap() < 0);
        assert_eq!(compare_ignore_case("ABC", "abd"), -1);
    }
}
