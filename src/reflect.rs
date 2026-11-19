//! Typed reflection for classes implemented in Rust: parameter types, return types, the class hierarchy and
//! `isAssignableFrom`, so engines that pick among overloads the way Java reflection users do (Velocity's
//! `MethodMap`) can see exactly what `Class.getMethods()` would show.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;

use crate::lang::{JError, JObject, JValue, ListKind, MapKind};

/// A Java type as it appears in a method signature.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum JType {
    /// `boolean`.
    Boolean,
    /// `byte`.
    Byte,
    /// `short`.
    Short,
    /// `char`.
    Char,
    /// `int`.
    Int,
    /// `long`.
    Long,
    /// `float`.
    Float,
    /// `double`.
    Double,
    /// A class or interface, by binary name (`java.lang.String`, `java.util.Map$Entry`).
    Class(Arc<str>),
    /// An array of the component type.
    Array(Arc<JType>),
}

/// `java.lang.Object`'s name.
pub const OBJECT: &str = "java.lang.Object";
/// `java.lang.String`'s name.
pub const STRING: &str = "java.lang.String";

impl JType {
    /// The class named `name`.
    pub fn class(name: &str) -> JType {
        JType::Class(name.into())
    }

    /// `java.lang.Object`.
    pub fn object() -> JType {
        JType::class(OBJECT)
    }

    /// `java.lang.String`.
    pub fn string() -> JType {
        JType::class(STRING)
    }

    /// An array of `component`.
    pub fn array(component: JType) -> JType {
        JType::Array(Arc::new(component))
    }

    /// `Class.isPrimitive()`.
    pub fn is_primitive(&self) -> bool {
        !matches!(self, JType::Class(_) | JType::Array(_))
    }

    /// `Class.isArray()`.
    pub fn is_array(&self) -> bool {
        matches!(self, JType::Array(_))
    }

    /// `Class.getComponentType()`: `None` unless an array.
    pub fn component(&self) -> Option<&JType> {
        match self {
            JType::Array(c) => Some(c),
            _ => None,
        }
    }

    /// `Type.getTypeName()`: `int`, `java.lang.String`, `java.lang.Object[]`.
    pub fn type_name(&self) -> String {
        match self {
            JType::Array(c) => format!("{}[]", c.type_name()),
            JType::Class(n) => n.to_string(),
            p => p.primitive_name().to_string(),
        }
    }

    fn primitive_name(&self) -> &'static str {
        match self {
            JType::Boolean => "boolean",
            JType::Byte => "byte",
            JType::Short => "short",
            JType::Char => "char",
            JType::Int => "int",
            JType::Long => "long",
            JType::Float => "float",
            JType::Double => "double",
            _ => "",
        }
    }

    /// `Class.getName()`: arrays in descriptor form (`[Ljava.lang.String;`, `[I`).
    pub fn name(&self) -> String {
        match self {
            JType::Array(c) => format!("[{}", c.descriptor()),
            other => other.type_name(),
        }
    }

    fn descriptor(&self) -> String {
        match self {
            JType::Boolean => "Z".into(),
            JType::Byte => "B".into(),
            JType::Short => "S".into(),
            JType::Char => "C".into(),
            JType::Int => "I".into(),
            JType::Long => "J".into(),
            JType::Float => "F".into(),
            JType::Double => "D".into(),
            JType::Class(n) => format!("L{n};"),
            JType::Array(c) => format!("[{}", c.descriptor()),
        }
    }

    /// `Class.getSimpleName()`.
    pub fn simple_name(&self) -> String {
        match self {
            JType::Array(c) => format!("{}[]", c.simple_name()),
            JType::Class(n) => {
                let tail = n.rsplit('.').next().unwrap_or(n);
                tail.rsplit('$').next().unwrap_or(tail).to_string()
            }
            p => p.primitive_name().to_string(),
        }
    }

    /// The wrapper class of a primitive (`IntrospectionUtils.getBoxedClass`); other types unchanged.
    pub fn boxed(&self) -> JType {
        let name = match self {
            JType::Boolean => "java.lang.Boolean",
            JType::Byte => "java.lang.Byte",
            JType::Short => "java.lang.Short",
            JType::Char => "java.lang.Character",
            JType::Int => "java.lang.Integer",
            JType::Long => "java.lang.Long",
            JType::Float => "java.lang.Float",
            JType::Double => "java.lang.Double",
            other => return other.clone(),
        };
        JType::class(name)
    }

    /// The primitive of a wrapper class (`IntrospectionUtils.getUnboxedClass`); other types unchanged.
    pub fn unboxed(&self) -> JType {
        match self {
            JType::Class(n) => match &**n {
                "java.lang.Boolean" => JType::Boolean,
                "java.lang.Byte" => JType::Byte,
                "java.lang.Short" => JType::Short,
                "java.lang.Character" => JType::Char,
                "java.lang.Integer" => JType::Int,
                "java.lang.Long" => JType::Long,
                "java.lang.Float" => JType::Float,
                "java.lang.Double" => JType::Double,
                _ => self.clone(),
            },
            _ => self.clone(),
        }
    }

    /// `Class.isAssignableFrom(actual)`.
    pub fn is_assignable_from(&self, actual: &JType) -> bool {
        if self == actual {
            return true;
        }
        match (self, actual) {
            (_, _) if self.is_primitive() || actual.is_primitive() => false,
            (JType::Class(f), _) if &**f == OBJECT => true,
            (JType::Class(f), JType::Array(_)) => matches!(&**f, "java.lang.Cloneable" | "java.io.Serializable"),
            (JType::Array(f), JType::Array(a)) => !f.is_primitive() && !a.is_primitive() && f.is_assignable_from(a),
            (JType::Class(f), JType::Class(a)) => supertypes(a).iter().any(|s| s == f),
            _ => false,
        }
    }
}

impl fmt::Debug for JType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.type_name())
    }
}

/// Calls a method: the target (`null` for a static method) and the arguments, already converted to the
/// parameter types (varargs packed into an array). `Ok(None)` for `void`.
pub type Invoker = Arc<dyn Fn(&JValue, &[JValue]) -> Result<Option<JValue>, JError> + Send + Sync>;

/// A public method as `Class.getMethods()` returns it.
#[derive(Clone)]
pub struct JMethod {
    /// `getName()`.
    pub name: Arc<str>,
    /// `getParameterTypes()`.
    pub params: Vec<JType>,
    /// `None` for `void`.
    pub returns: Option<JType>,
    /// `getDeclaringClass().getName()`.
    pub declaring_class: Arc<str>,
    /// Whether it is `static`.
    pub is_static: bool,
    /// Declared with `...` (`Method.isVarArgs()`); an array last parameter counts as varargs for Velocity anyway.
    pub varargs: bool,
    /// Calls the method on a receiver with arguments; `Ok(None)` for `void`.
    pub invoke: Invoker,
}

impl JMethod {
    /// `Method.toString()`-like signature, for messages.
    pub fn signature(&self) -> String {
        let params: Vec<String> = self.params.iter().map(JType::type_name).collect();
        format!("{}.{}({})", self.declaring_class, self.name, params.join(","))
    }
}

impl fmt::Debug for JMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.signature())
    }
}

/// A public field (`UberspectPublicFields`, `ClassTool`).
#[derive(Clone)]
pub struct JField {
    /// `getName()`.
    pub name: Arc<str>,
    /// `getType()`.
    pub field_type: JType,
    /// Whether it is `static`.
    pub is_static: bool,
    /// Reads the field of a receiver (`Field.get`).
    pub get: Arc<dyn Fn(&JValue) -> JValue + Send + Sync>,
}

/// A class as reflection sees it.
#[derive(Clone)]
pub struct ClassDef {
    /// `getName()`.
    pub name: Arc<str>,
    /// Superclasses nearest first, `java.lang.Object` excluded.
    pub superclasses: Vec<Arc<str>>,
    /// Every interface implemented directly or inherited.
    pub interfaces: Vec<Arc<str>>,
    /// Whether the class is `public`.
    pub public: bool,
    /// `isInterface()`.
    pub is_interface: bool,
    /// Public methods, inherited ones included, in declaration order.
    pub methods: Vec<JMethod>,
    /// Public fields, inherited ones included.
    pub fields: Vec<JField>,
    /// Public constructors.
    pub constructors: Vec<JMethod>,
    /// `isAnnotationPresent(Deprecated.class)` on the class.
    pub deprecated: bool,
    /// The methods annotated `@Deprecated`, by name.
    pub deprecated_methods: Vec<Arc<str>>,
    /// An `enum`'s constants, in declaration order (`isEnum()` when not empty).
    pub enum_constants: Vec<Arc<str>>,
}

impl fmt::Debug for ClassDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "class {}", self.name)
    }
}

impl ClassDef {
    /// A public class named `name`, with no supertypes or members yet.
    pub fn new(name: &str) -> ClassDef {
        ClassDef {
            name: name.into(),
            superclasses: Vec::new(),
            interfaces: Vec::new(),
            public: true,
            is_interface: false,
            methods: Vec::new(),
            fields: Vec::new(),
            constructors: Vec::new(),
            deprecated: false,
            deprecated_methods: Vec::new(),
            enum_constants: Vec::new(),
        }
    }

    /// An `enum` with these constants.
    pub fn enum_of(mut self, constants: &[&str]) -> ClassDef {
        self.enum_constants = constants.iter().map(|c| Arc::from(*c)).collect();
        self
    }

    /// `isEnum()`: whether it has enum constants.
    pub fn is_enum(&self) -> bool {
        !self.enum_constants.is_empty()
    }

    /// `@Deprecated` on the class.
    pub fn deprecated(mut self) -> ClassDef {
        self.deprecated = true;
        self
    }

    /// `@Deprecated` on the methods named `name`.
    pub fn deprecated_method(mut self, name: &str) -> ClassDef {
        self.deprecated_methods.push(name.into());
        self
    }

    /// Its superclasses, nearest first.
    pub fn extends(mut self, superclasses: &[&str]) -> ClassDef {
        self.superclasses = superclasses.iter().map(|s| Arc::from(*s)).collect();
        self
    }

    /// The interfaces it implements, inherited ones included.
    pub fn implements(mut self, interfaces: &[&str]) -> ClassDef {
        self.interfaces = interfaces.iter().map(|s| Arc::from(*s)).collect();
        self
    }

    /// Marks the class not `public`.
    pub fn non_public(mut self) -> ClassDef {
        self.public = false;
        self
    }

    /// Marks it an interface.
    pub fn interface(mut self) -> ClassDef {
        self.is_interface = true;
        self
    }

    /// A public instance method declared on this class.
    pub fn method<T: 'static>(
        self,
        name: &str,
        params: &[JType],
        returns: Option<JType>,
        f: impl Fn(&T, &JValue, &[JValue]) -> Result<Option<JValue>, JError> + Send + Sync + 'static,
    ) -> ClassDef {
        let declaring = self.name.clone();
        self.method_of(&declaring, name, params, returns, f)
    }

    /// A public instance method `declaring` declares and this class inherits.
    pub fn method_of<T: 'static>(
        mut self,
        declaring: &str,
        name: &str,
        params: &[JType],
        returns: Option<JType>,
        f: impl Fn(&T, &JValue, &[JValue]) -> Result<Option<JValue>, JError> + Send + Sync + 'static,
    ) -> ClassDef {
        let varargs = params.last().is_some_and(JType::is_array);
        let invoke: Invoker = Arc::new(move |target, args| match target {
            JValue::Object(o) => match o.as_any().downcast_ref::<T>() {
                Some(t) => f(t, target, args),
                None => Err(JError::new("java.lang.IllegalArgumentException", "object is not an instance of declaring class")),
            },
            _ => Err(JError::new("java.lang.IllegalArgumentException", "object is not an instance of declaring class")),
        });
        self.methods.push(JMethod { name: name.into(), params: params.to_vec(), returns, declaring_class: declaring.into(), is_static: false, varargs, invoke });
        self
    }

    /// Any method, target handled by the invoker itself (static methods, methods of value types).
    pub fn raw_method(mut self, method: JMethod) -> ClassDef {
        self.methods.push(method);
        self
    }

    /// Adds a public field.
    pub fn field(mut self, field: JField) -> ClassDef {
        self.fields.push(field);
        self
    }

    /// Makes the class known to [`supertypes`] (and so to `isAssignableFrom`) under its name.
    pub fn register(self) -> Arc<ClassDef> {
        let def = Arc::new(self);
        REGISTRY.write().insert(def.name.to_string(), def.clone());
        def
    }

    /// The class as a type.
    pub fn jtype(&self) -> JType {
        JType::Class(self.name.clone())
    }

    /// `getMethods()` filtered by name.
    pub fn methods_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a JMethod> + 'a {
        self.methods.iter().filter(move |m| &*m.name == name)
    }
}

static REGISTRY: LazyLock<RwLock<HashMap<String, Arc<ClassDef>>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// A registered class by name.
pub fn class_named(name: &str) -> Option<Arc<ClassDef>> {
    REGISTRY.read().get(name).cloned()
}

const NUMBER_SUPERS: &[&str] = &["java.lang.Number", "java.io.Serializable", "java.lang.Comparable", "java.lang.constant.Constable", "java.lang.constant.ConstantDesc"];
const LIST_SUPERS: &[&str] = &[
    "java.util.AbstractList",
    "java.util.AbstractCollection",
    "java.util.List",
    "java.util.SequencedCollection",
    "java.util.Collection",
    "java.lang.Iterable",
    "java.util.RandomAccess",
    "java.lang.Cloneable",
    "java.io.Serializable",
];

/// The JDK classes [`JValue`] models: superclasses then interfaces, transitively, `Object` excluded.
fn builtin_supertypes(class: &str) -> Option<&'static [&'static str]> {
    Some(match class {
        "java.lang.String" => &["java.io.Serializable", "java.lang.Comparable", "java.lang.CharSequence", "java.lang.constant.Constable", "java.lang.constant.ConstantDesc"],
        "java.lang.Integer" | "java.lang.Long" | "java.lang.Double" | "java.lang.Float" | "java.lang.Short" | "java.lang.Byte" => NUMBER_SUPERS,
        "java.math.BigInteger" | "java.math.BigDecimal" => &["java.lang.Number", "java.io.Serializable", "java.lang.Comparable"],
        "java.lang.Number" => &["java.io.Serializable"],
        "java.lang.Boolean" | "java.lang.Character" => &["java.io.Serializable", "java.lang.Comparable", "java.lang.constant.Constable"],
        "java.util.ArrayList" => LIST_SUPERS,
        "net.minidev.json.JSONArray" => &[
            "java.util.ArrayList",
            "java.util.AbstractList",
            "java.util.AbstractCollection",
            "java.util.List",
            "java.util.SequencedCollection",
            "java.util.Collection",
            "java.lang.Iterable",
            "java.util.RandomAccess",
            "java.lang.Cloneable",
            "java.io.Serializable",
            "net.minidev.json.JSONAware",
            "net.minidev.json.JSONAwareEx",
            "net.minidev.json.JSONStreamAware",
            "net.minidev.json.JSONStreamAwareEx",
        ],
        "java.util.Arrays$ArrayList" => &["java.util.AbstractList", "java.util.AbstractCollection", "java.util.List", "java.util.SequencedCollection", "java.util.Collection", "java.lang.Iterable", "java.util.RandomAccess", "java.io.Serializable"],
        "java.util.LinkedHashSet" => &[
            "java.util.HashSet",
            "java.util.AbstractSet",
            "java.util.AbstractCollection",
            "java.util.SequencedSet",
            "java.util.SequencedCollection",
            "java.util.Set",
            "java.util.Collection",
            "java.lang.Iterable",
            "java.lang.Cloneable",
            "java.io.Serializable",
        ],
        "java.util.LinkedHashMap$LinkedValues" => &["java.util.AbstractCollection", "java.util.SequencedCollection", "java.util.Collection", "java.lang.Iterable"],
        "java.util.ArrayList$Itr" => &["java.util.Iterator"],
        "java.util.HashMap" => &["java.util.AbstractMap", "java.util.Map", "java.lang.Cloneable", "java.io.Serializable"],
        "java.util.LinkedHashMap" => &["java.util.HashMap", "java.util.AbstractMap", "java.util.SequencedMap", "java.util.Map", "java.lang.Cloneable", "java.io.Serializable"],
        _ => return None,
    })
}

/// Every superclass and interface of `class` (`Object` excluded): the JDK types [`JValue`] models, then any
/// registered [`ClassDef`].
pub fn supertypes(class: &str) -> Vec<Arc<str>> {
    if let Some(s) = builtin_supertypes(class) {
        return s.iter().map(|s| Arc::from(*s)).collect();
    }
    match class_named(class) {
        Some(def) => supertypes_of(&def),
        None => Vec::new(),
    }
}

fn supertypes_of(def: &ClassDef) -> Vec<Arc<str>> {
    let mut out: Vec<Arc<str>> = Vec::new();
    let push = |s: &Arc<str>, out: &mut Vec<Arc<str>>| {
        if !out.contains(s) {
            out.push(s.clone());
        }
    };
    for s in def.superclasses.iter().chain(&def.interfaces) {
        push(s, &mut out);
        for t in supertypes(s) {
            push(&t, &mut out);
        }
    }
    out
}

/// `value.getClass()`: `None` for `null`.
pub fn class_of(v: &JValue) -> Option<JType> {
    Some(match v {
        JValue::Null => return None,
        JValue::Array(a) => JType::array(a.component.clone()),
        JValue::List(l) if l.kind() == ListKind::ArrayList => JType::class("java.util.ArrayList"),
        JValue::Map(m) if m.kind() == MapKind::Hash => JType::class("java.util.HashMap"),
        other => JType::Class(other.class_name().into()),
    })
}

/// The class definition of an object, when it has one.
pub fn class_def(v: &JValue) -> Option<Arc<ClassDef>> {
    match v {
        JValue::Object(o) => o.class_def(),
        _ => None,
    }
}

/// Convenience for [`JObject::class_def`] implementations: a lazily built, registered definition.
pub struct LazyClass(LazyLock<Arc<ClassDef>, fn() -> Arc<ClassDef>>);

impl LazyClass {
    /// A definition `build` makes on first use.
    pub const fn new(build: fn() -> Arc<ClassDef>) -> LazyClass {
        LazyClass(LazyLock::new(build))
    }

    /// The definition, built on first use.
    pub fn get(&self) -> Arc<ClassDef> {
        self.0.clone()
    }
}

/// `true` for `JObject`s whose class is `T`.
pub fn is<T: 'static>(v: &JValue) -> bool {
    matches!(v, JValue::Object(o) if o.as_any().is::<T>())
}

/// Downcasts an object argument.
pub fn downcast<T: 'static>(v: &JValue) -> Option<&T> {
    match v {
        JValue::Object(o) => o.as_any().downcast_ref::<T>(),
        _ => None,
    }
}

/// The arguments `Object.toString()` of a `JObject` without its own `toString` gives: `name@hash`.
pub fn identity_string(o: &dyn JObject) -> String {
    let hash = (o as *const dyn JObject as *const () as usize) as u32;
    format!("{}@{hash:x}", o.class_name())
}

/// A constant of an `enum` class: `name()`, `ordinal()`, and `toString()` as its name.
pub struct EnumConstant {
    /// The enum class.
    pub class: Arc<ClassDef>,
    /// `name()`.
    pub name: Arc<str>,
    /// `ordinal()`.
    pub ordinal: i32,
}

impl EnumConstant {
    /// `Enum.valueOf(enumClass, name)`.
    pub fn value_of(class: &Arc<ClassDef>, name: &str) -> Result<JValue, JError> {
        match class.enum_constants.iter().position(|c| &**c == name) {
            Some(i) => Ok(JValue::Object(Arc::new(EnumConstant { class: class.clone(), name: class.enum_constants[i].clone(), ordinal: i as i32 }))),
            None => Err(JError::new("java.lang.IllegalArgumentException", format!("No enum constant {}.{name}", class.name.replace('$', ".")))),
        }
    }
}

impl crate::lang::JObject for EnumConstant {
    fn class_name(&self) -> String {
        self.class.name.to_string()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        match (name, args) {
            ("name", []) | ("toString", []) => Some(Ok(Some(JValue::str(&*self.name)))),
            ("ordinal", []) => Some(Ok(Some(JValue::Int(self.ordinal)))),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        self.name.to_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignability_follows_the_jdk_hierarchy() {
        let number = JType::class("java.lang.Number");
        assert!(number.is_assignable_from(&JType::class("java.lang.Integer")));
        assert!(!JType::string().is_assignable_from(&JType::class("java.lang.Integer")));
        assert!(JType::object().is_assignable_from(&JType::array(JType::Int)));
        assert!(!JType::object().is_assignable_from(&JType::Int));
        assert!(JType::array(JType::object()).is_assignable_from(&JType::array(JType::string())));
        assert!(!JType::array(JType::object()).is_assignable_from(&JType::array(JType::Int)));
        assert!(JType::class("java.util.Collection").is_assignable_from(&JType::class("java.util.ArrayList")));
    }

    #[test]
    fn names() {
        assert_eq!(JType::array(JType::string()).name(), "[Ljava.lang.String;");
        assert_eq!(JType::array(JType::string()).type_name(), "java.lang.String[]");
        assert_eq!(JType::class("java.util.Map$Entry").simple_name(), "Entry");
        assert_eq!(JType::Int.boxed(), JType::class("java.lang.Integer"));
        assert_eq!(JType::class("java.lang.Integer").unboxed(), JType::Int);
    }

    #[test]
    fn registered_classes_extend_the_hierarchy() {
        ClassDef::new("x.Base").implements(&["java.io.Serializable"]).register();
        ClassDef::new("x.Sub").extends(&["x.Base"]).register();
        assert!(JType::class("x.Base").is_assignable_from(&JType::class("x.Sub")));
        assert!(JType::class("java.io.Serializable").is_assignable_from(&JType::class("x.Sub")));
    }
}
