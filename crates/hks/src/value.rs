//! Script values. HKS numbers are 32-bit floats; strings are byte strings.

use crate::table::TableRef;
use crate::vm::{Builtin, LuaClosure};
use sekiro_formats::hks::format_number;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// An immutable byte string.
pub type LStr = Rc<[u8]>;

#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Number(f32),
    Str(LStr),
    Table(TableRef),
    Function(Function),
}

#[derive(Clone)]
pub enum Function {
    Lua(Rc<LuaClosure>),
    Builtin(Builtin),
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s.as_bytes()))
    }

    pub fn bytes(b: &[u8]) -> Value {
        Value::Str(Rc::from(b))
    }

    pub fn from_bool(b: bool) -> Value {
        Value::Bool(b)
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    /// Lua truthiness: everything except `nil` and `false`.
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn as_number(&self) -> Option<f32> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Str(s) => std::str::from_utf8(s).ok()?.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => std::str::from_utf8(s).ok(),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Table(_) => "table",
            Value::Function(_) => "function",
        }
    }

    /// Primitive equality (`rawequal`): by value for scalars and strings, by identity otherwise.
    pub fn raw_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => Rc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => a.ptr_eq(b),
            _ => false,
        }
    }

    /// Text for `tostring` and concatenation.
    pub fn to_display(&self) -> String {
        match self {
            Value::Nil => "nil".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => format_number(*n),
            Value::Str(s) => String::from_utf8_lossy(s).into_owned(),
            Value::Table(t) => format!("table: {:p}", Rc::as_ptr(t)),
            Value::Function(f) => match f {
                Function::Lua(c) => format!("function: {}", c.proto.name),
                Function::Builtin(b) => format!("builtin: {b:?}"),
            },
        }
    }
}

impl Function {
    fn ptr_eq(&self, other: &Function) -> bool {
        match (self, other) {
            (Function::Lua(a), Function::Lua(b)) => Rc::ptr_eq(a, b),
            (Function::Builtin(a), Function::Builtin(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{:?}", String::from_utf8_lossy(s)),
            other => f.write_str(&other.to_display()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_display())
    }
}

impl From<f32> for Value {
    fn from(n: f32) -> Self {
        Value::Number(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Number(n as f32)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::str(s)
    }
}

/// A table key: a non-nil, non-NaN value hashed by content or identity.
#[derive(Clone, Debug)]
pub struct Key(pub Value);

impl Key {
    /// Normalises `-0.0` so it hashes like `0.0`. Returns `None` for nil and NaN.
    pub fn new(v: Value) -> Option<Key> {
        match v {
            Value::Nil => None,
            Value::Number(n) if n.is_nan() => None,
            // Matches -0.0 too, so both zeros hash alike.
            Value::Number(0.0) => Some(Key(Value::Number(0.0))),
            other => Some(Key(other)),
        }
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.0.raw_eq(&other.0)
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            Value::Nil => 0u8.hash(state),
            Value::Bool(b) => (1u8, b).hash(state),
            Value::Number(n) => (2u8, n.to_bits()).hash(state),
            Value::Str(s) => (3u8, &**s).hash(state),
            Value::Table(t) => (4u8, Rc::as_ptr(t) as *const u8 as usize).hash(state),
            Value::Function(Function::Lua(c)) => {
                (5u8, Rc::as_ptr(c) as *const u8 as usize).hash(state)
            }
            Value::Function(Function::Builtin(b)) => (6u8, b).hash(state),
        }
    }
}
