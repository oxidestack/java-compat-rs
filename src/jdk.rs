//! JDK classes other than the value types [`JValue`] models directly, as objects with typed definitions
//! ([`ClassDef`]) the way reflection sees them.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::lang::{JError, JObject, JValue};
use crate::reflect::{ClassDef, JType, LazyClass};

fn ret(v: JValue) -> Result<Option<JValue>, JError> {
    Ok(Some(v))
}

/// `java.lang.StringBuilder`, as UTF-16 code units.
pub struct StringBuilder {
    units: Mutex<Vec<u16>>,
}

impl StringBuilder {
    /// `new StringBuilder(initial)`.
    pub fn new(initial: &str) -> Arc<StringBuilder> {
        Arc::new(StringBuilder { units: Mutex::new(initial.encode_utf16().collect()) })
    }

    /// `new StringBuilder(initial)` as a Java value.
    pub fn value(initial: &str) -> JValue {
        JValue::Object(StringBuilder::new(initial))
    }

    /// `append(s)`.
    pub fn append(&self, s: &str) {
        self.units.lock().extend(s.encode_utf16());
    }

    /// `toString()`; an unpaired surrogate becomes U+FFFD.
    pub fn text(&self) -> String {
        String::from_utf16_lossy(&self.units.lock())
    }
}

fn string_builder_def() -> Arc<ClassDef> {
    let sb = || JType::class("java.lang.StringBuilder");
    let def = ClassDef::new("java.lang.StringBuilder")
        .extends(&["java.lang.AbstractStringBuilder"])
        .implements(&["java.lang.Appendable", "java.lang.CharSequence", "java.lang.Comparable", "java.io.Serializable"])
        .method::<StringBuilder>("append", &[JType::object()], Some(sb()), |b, this, a| {
            b.append(&a[0].to_java_string());
            ret(this.clone())
        })
        .method::<StringBuilder>("append", &[JType::string()], Some(sb()), |b, this, a| {
            b.append(&a[0].to_java_string());
            ret(this.clone())
        })
        .method::<StringBuilder>("toString", &[], Some(JType::string()), |b, _, _| ret(JValue::str(b.text())))
        .method_of::<StringBuilder>("java.lang.AbstractStringBuilder", "length", &[], Some(JType::Int), |b, _, _| ret(JValue::Int(b.units.lock().len() as i32)))
        .method_of::<StringBuilder>("java.lang.CharSequence", "isEmpty", &[], Some(JType::Boolean), |b, _, _| ret(JValue::Bool(b.units.lock().is_empty())));
    def.register()
}

impl JObject for StringBuilder {
    fn class_name(&self) -> String {
        "java.lang.StringBuilder".into()
    }

    fn invoke(&self, name: &str, args: &[JValue]) -> Option<Result<Option<JValue>, JError>> {
        match (name, args) {
            ("length", []) => Some(ret(JValue::Int(self.units.lock().len() as i32))),
            ("isEmpty", []) => Some(ret(JValue::Bool(self.units.lock().is_empty()))),
            ("toString", []) => Some(ret(JValue::str(self.text()))),
            _ => None,
        }
    }

    fn to_java_string(&self) -> String {
        self.text()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn class_def(&self) -> Option<Arc<ClassDef>> {
        static DEF: LazyClass = LazyClass::new(string_builder_def);
        Some(DEF.get())
    }
}
