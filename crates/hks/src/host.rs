//! The engine side of the script interface.
//!
//! The scripts query the engine with `env(id, ...)`, command it with `act(id, ...)` and drive
//! the Havok behaviour graph through `hkb*` functions. A simulation implements [`Host`] to
//! answer them; see `docs/HKS-INTERFACE.md` for what each id is known to mean.

use crate::value::Value;
use std::fmt;

/// The first argument of `env`/`act`: a numeric id, or a Japanese command name that the engine
/// resolves to an id through its own name table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandId<'a> {
    Number(i32),
    Name(&'a str),
}

impl fmt::Display for CommandId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandId::Number(n) => write!(f, "{n}"),
            CommandId::Name(s) => write!(f, "{s:?}"),
        }
    }
}

/// Engine globals the scripts call that are not part of the Lua standard library.
///
/// `env` and `act` are routed to [`Host::env`] and [`Host::act`]; everything else goes to
/// [`Host::call`] with its global name.
///
/// This is every global the 81 shipped scripts read without defining that is a function the
/// engine must supply (measured with `hks-inventory`). Undefined globals not listed here
/// resolve to the scripts' own `dummy` no-op through the `_G` metatable, as in the game.
pub const ENGINE_FUNCTIONS: &[&str] = &[
    "env",
    "act",
    "hkbFireEvent",
    "hkbGetVariable",
    "hkbSetVariable",
    "hkbIsNodeActive",
    "hkbIsBoneValid",
    "hkbGetBoneModelSpace",
    "hkbGetOldBoneModelSpace",
    "hkbSetBoneModelSpace",
];

pub trait Host {
    /// A condition query, `env(id, args...)`. Must return exactly one value.
    fn env(&mut self, id: CommandId<'_>, args: &[Value]) -> Value;

    /// A command, `act(id, args...)`. Most commands return nothing (nil).
    fn act(&mut self, id: CommandId<'_>, args: &[Value]) -> Value;

    /// Any other engine function, such as `hkbFireEvent(name)` or `hkbGetVariable(name)`.
    fn call(&mut self, name: &str, args: &[Value]) -> Vec<Value>;
}

/// One recorded engine call.
#[derive(Debug, Clone)]
pub struct CallRecord {
    pub function: String,
    pub args: Vec<Value>,
    pub result: Vec<Value>,
}

impl fmt::Display for CallRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.function)?;
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{a:?}")?;
        }
        f.write_str(")")?;
        if !self.result.is_empty() {
            f.write_str(" -> ")?;
            for (i, r) in self.result.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{r:?}")?;
            }
        }
        Ok(())
    }
}

/// A stub host for bring-up: answers every query with a fixed default and logs every call.
///
/// `env` returns `0` (which the scripts compare against `FALSE`), behaviour variables return
/// `0`, `hkbIsNodeActive` returns `false`. Individual answers can be overridden with
/// [`LoggingHost::set_env`], matched on the id and, when given, the first argument.
#[derive(Default)]
pub struct LoggingHost {
    pub log: Vec<CallRecord>,
    pub env_overrides: Vec<(i32, Option<f32>, Value)>,
    pub variables: std::collections::HashMap<String, Value>,
}

impl LoggingHost {
    /// Makes `env(id)` (or `env(id, arg)`) return `value`. Later calls take precedence.
    pub fn set_env(&mut self, id: i32, arg: Option<f32>, value: impl Into<Value>) {
        self.env_overrides.insert(0, (id, arg, value.into()));
    }

    fn record(&mut self, function: String, args: &[Value], result: Vec<Value>) -> Vec<Value> {
        self.log.push(CallRecord {
            function,
            args: args.to_vec(),
            result: result.clone(),
        });
        result
    }
}

impl Host for LoggingHost {
    fn env(&mut self, id: CommandId<'_>, args: &[Value]) -> Value {
        let value = match id {
            CommandId::Number(n) => {
                let first = args.first().and_then(Value::as_number);
                self.env_overrides
                    .iter()
                    .find(|(k, arg, _)| *k == n && (arg.is_none() || *arg == first))
                    .map(|(_, _, v)| v.clone())
                    .unwrap_or(Value::Number(0.0))
            }
            CommandId::Name(_) => Value::Number(0.0),
        };
        let mut all = vec![command_value(id)];
        all.extend_from_slice(args);
        self.record("env".into(), &all, vec![value.clone()]);
        value
    }

    fn act(&mut self, id: CommandId<'_>, args: &[Value]) -> Value {
        let mut all = vec![command_value(id)];
        all.extend_from_slice(args);
        self.record("act".into(), &all, Vec::new());
        Value::Nil
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Vec<Value> {
        let result = match name {
            "hkbGetVariable" => {
                let key = args.first().map(Value::to_display).unwrap_or_default();
                vec![
                    self.variables
                        .get(&key)
                        .cloned()
                        .unwrap_or(Value::Number(0.0)),
                ]
            }
            "hkbSetVariable" => {
                if let (Some(k), Some(v)) = (args.first(), args.get(1)) {
                    self.variables.insert(k.to_display(), v.clone());
                }
                Vec::new()
            }
            "hkbIsNodeActive" => vec![Value::Bool(false)],
            _ => Vec::new(),
        };
        self.record(name.into(), args, result)
    }
}

fn command_value(id: CommandId<'_>) -> Value {
    match id {
        CommandId::Number(n) => Value::Number(n as f32),
        CommandId::Name(s) => Value::str(s),
    }
}
