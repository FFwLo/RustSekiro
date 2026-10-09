//! A small Lua 5.0 virtual machine for the game's compiled AI scripts.
//!
//! Adapted from sekiro-rs (https://github.com/AKJama/sekiro-rs),
//! `crates/sim/src/lua_ai/lua50.rs`, Copyright (c) 2026 AKJama, used under the MIT license
//! (sekiro-rs is dual-licensed MIT or Apache-2.0). Changes: no `thiserror`, a `next` builtin.
//!
//! The AI scripts (`script/aicommon.luabnd`, `script/mXX_XX_XX_XX.luabnd`) are standard Lua 5.0
//! bytecode ("\x1bLua", version 0x50) with 8-byte `size_t` and `double` numbers. This module
//! loads that format and interprets the 35 Lua 5.0 opcodes. It is written from the published
//! Lua 5.0 format and instruction set; only what the AI scripts need is supported (no
//! metatables, coroutines or string library).
//!
//! Engine objects (the AI and goal handles the scripts receive) are [`Value::Obj`]; indexing one
//! with a string yields a [`Value::Method`] that the [`Host`] executes. Engine functions called
//! by name are [`Value::Host`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// An engine object handle (kind, id), owned by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Obj {
    pub kind: u8,
    pub id: u32,
}

#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Table(TableRef),
    Func(Rc<Closure>),
    /// An engine function called by global name.
    Host(Rc<str>),
    /// An engine object.
    Obj(Obj),
    /// An engine object's method, produced by indexing the object.
    Method(Obj, Rc<str>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Table(t) => write!(f, "table@{:p}", Rc::as_ptr(t)),
            Value::Func(c) => write!(f, "function@{:p}", Rc::as_ptr(c)),
            Value::Host(n) => write!(f, "host:{n}"),
            Value::Obj(o) => write!(f, "obj{}:{}", o.kind, o.id),
            Value::Method(o, n) => write!(f, "obj{}:{}.{n}", o.kind, o.id),
        }
    }
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Table(_) => "table",
            Value::Func(_) | Value::Host(_) | Value::Method(..) => "function",
            Value::Obj(_) => "userdata",
        }
    }

    fn raw_eq(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => Rc::ptr_eq(a, b),
            (Value::Func(a), Value::Func(b)) => Rc::ptr_eq(a, b),
            (Value::Host(a), Value::Host(b)) => a == b,
            (Value::Obj(a), Value::Obj(b)) => a == b,
            (Value::Method(a, x), Value::Method(b, y)) => a == b && x == y,
            _ => false,
        }
    }

    fn to_concat(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.to_string()),
            Value::Num(n) => Some(fmt_num(*n)),
            _ => None,
        }
    }
}

/// Lua 5.0 prints numbers with `%.14g`.
pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Bool(bool),
    Num(u64),
    Str(Rc<str>),
    Ptr(usize),
    Host(Rc<str>),
    Obj(Obj),
}

fn key_of(v: &Value) -> Option<Key> {
    Some(match v {
        Value::Nil => return None,
        Value::Bool(b) => Key::Bool(*b),
        Value::Num(n) => {
            if n.is_nan() {
                return None;
            }
            Key::Num(if *n == 0.0 { 0 } else { n.to_bits() })
        }
        Value::Str(s) => Key::Str(s.clone()),
        Value::Table(t) => Key::Ptr(Rc::as_ptr(t) as *const u8 as usize),
        Value::Func(c) => Key::Ptr(Rc::as_ptr(c) as *const u8 as usize),
        Value::Host(n) | Value::Method(_, n) => Key::Host(n.clone()),
        Value::Obj(o) => Key::Obj(*o),
    })
}

/// A Lua table: insertion-ordered entries with a key index.
#[derive(Debug, Default)]
pub struct Table {
    entries: Vec<(Value, Value)>,
    index: HashMap<Key, usize>,
}

pub type TableRef = Rc<RefCell<Table>>;

impl Table {
    pub fn new_ref() -> TableRef {
        Rc::new(RefCell::new(Table::default()))
    }

    pub fn get(&self, k: &Value) -> Value {
        key_of(k)
            .and_then(|k| self.index.get(&k))
            .map(|&i| self.entries[i].1.clone())
            .unwrap_or_default()
    }

    pub fn get_str(&self, k: &str) -> Value {
        self.get(&Value::str(k))
    }

    pub fn set(&mut self, k: Value, v: Value) {
        let Some(key) = key_of(&k) else { return };
        match self.index.get(&key) {
            Some(&i) => self.entries[i].1 = v,
            None => {
                if v.is_nil() {
                    return;
                }
                self.index.insert(key, self.entries.len());
                self.entries.push((k, v));
            }
        }
    }

    /// `table.getn`: the count of consecutive integer keys from 1.
    pub fn len(&self) -> usize {
        let mut n = 0;
        while !self.get(&Value::Num((n + 1) as f64)).is_nil() {
            n += 1;
        }
        n
    }

    /// The entry after `k` (nil for the first), skipping removed entries.
    pub fn next(&self, k: &Value) -> Option<(Value, Value)> {
        let start = if k.is_nil() {
            0
        } else {
            key_of(k).and_then(|k| self.index.get(&k)).map(|i| i + 1)?
        };
        self.entries[start..]
            .iter()
            .find(|(_, v)| !v.is_nil())
            .cloned()
    }
}

impl Value {
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }
}

/// One compiled function.
#[derive(Debug)]
pub struct Proto {
    pub source: String,
    pub nups: u8,
    pub params: u8,
    pub vararg: bool,
    pub max_stack: u8,
    pub code: Vec<u32>,
    pub consts: Vec<Value>,
    pub protos: Vec<Rc<Proto>>,
    pub lines: Vec<i32>,
}

#[derive(Debug)]
enum Upval {
    Open(usize),
    Closed(Value),
}

type UpRef = Rc<RefCell<Upval>>;

pub struct Closure {
    proto: Rc<Proto>,
    upvals: Vec<UpRef>,
}

#[derive(Debug)]
pub enum LuaError {
    Chunk(String),
    Runtime(String),
}

impl fmt::Display for LuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaError::Chunk(m) => write!(f, "bad chunk: {m}"),
            LuaError::Runtime(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LuaError {}

pub type LuaResult<T> = Result<T, LuaError>;

fn rt<T>(msg: impl Into<String>) -> LuaResult<T> {
    Err(LuaError::Runtime(msg.into()))
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> LuaResult<&[u8]> {
        if self.p + n > self.b.len() {
            return Err(LuaError::Chunk("truncated".into()));
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> LuaResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn int(&mut self) -> LuaResult<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn size(&mut self) -> LuaResult<usize> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()) as usize)
    }
    fn num(&mut self) -> LuaResult<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn string(&mut self) -> LuaResult<Option<String>> {
        let n = self.size()?;
        if n == 0 {
            return Ok(None);
        }
        let s = self.take(n)?;
        Ok(Some(String::from_utf8_lossy(&s[..n - 1]).into_owned()))
    }
}

/// Parses a Lua 5.0 binary chunk (little endian, 4-byte int, 8-byte size_t and number).
pub fn load_chunk(data: &[u8]) -> LuaResult<Rc<Proto>> {
    if data.len() < 22 || &data[..4] != b"\x1bLua" || data[4] != 0x50 {
        return Err(LuaError::Chunk("not a Lua 5.0 chunk".into()));
    }
    let want = [1u8, 4, 8, 4, 6, 8, 9, 9, 8];
    if data[5..14] != want {
        return Err(LuaError::Chunk(format!(
            "unsupported layout {:?}",
            &data[5..14]
        )));
    }
    let mut r = Reader { b: data, p: 22 };
    load_function(&mut r, "?")
}

fn load_function(r: &mut Reader, parent: &str) -> LuaResult<Rc<Proto>> {
    let source = r.string()?.unwrap_or_else(|| parent.to_owned());
    r.int()?; // line defined
    let nups = r.u8()?;
    let params = r.u8()?;
    let vararg = r.u8()? != 0;
    let max_stack = r.u8()?;
    let n = r.int()? as usize;
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        lines.push(r.int()?);
    }
    let n = r.int()? as usize;
    for _ in 0..n {
        r.string()?;
        r.int()?;
        r.int()?;
    }
    let n = r.int()? as usize;
    for _ in 0..n {
        r.string()?;
    }
    let n = r.int()? as usize;
    let mut consts = Vec::with_capacity(n);
    for _ in 0..n {
        consts.push(match r.u8()? {
            0 => Value::Nil,
            1 => Value::Bool(r.u8()? != 0),
            3 => Value::Num(r.num()?),
            4 => Value::Str(Rc::from(r.string()?.unwrap_or_default().as_str())),
            t => return Err(LuaError::Chunk(format!("constant type {t}"))),
        });
    }
    let n = r.int()? as usize;
    let mut protos = Vec::with_capacity(n);
    for _ in 0..n {
        protos.push(load_function(r, &source)?);
    }
    let n = r.int()? as usize;
    let mut code = Vec::with_capacity(n);
    for _ in 0..n {
        code.push(u32::from_le_bytes(r.take(4)?.try_into().unwrap()));
    }
    Ok(Rc::new(Proto {
        source,
        nups,
        params,
        vararg,
        max_stack,
        code,
        consts,
        protos,
        lines,
    }))
}

/// What the scripts call into: engine functions by name and engine object methods.
pub trait Host {
    fn call_host(&mut self, vm: &mut Vm, name: &str, args: &[Value]) -> LuaResult<Vec<Value>>;
    fn call_method(
        &mut self,
        vm: &mut Vm,
        obj: Obj,
        name: &str,
        args: &[Value],
    ) -> LuaResult<Vec<Value>>;
}

const MAXSTACK: u32 = 250;
const FIELDS_PER_FLUSH: usize = 32;

fn op(i: u32) -> u32 {
    i & 0x3f
}
fn arg_a(i: u32) -> usize {
    (i >> 24) as usize
}
fn arg_b(i: u32) -> u32 {
    (i >> 15) & 0x1ff
}
fn arg_c(i: u32) -> u32 {
    (i >> 6) & 0x1ff
}
fn arg_bx(i: u32) -> usize {
    ((i >> 6) & 0x3ffff) as usize
}
fn arg_sbx(i: u32) -> i64 {
    arg_bx(i) as i64 - 131_071
}

/// The interpreter: globals, the shared register stack and open upvalues.
pub struct Vm {
    pub globals: TableRef,
    stack: Vec<Value>,
    open: Vec<(usize, UpRef)>,
    depth: usize,
    /// Instructions executed (a runaway guard for tests).
    pub steps: u64,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Vm {
        let vm = Vm {
            globals: Table::new_ref(),
            stack: Vec::new(),
            open: Vec::new(),
            depth: 0,
            steps: 0,
        };
        for name in ["print", "loadstring", "next"] {
            vm.set_global(name, Value::Host(Rc::from(name)));
        }
        let math = Table::new_ref();
        for f in [
            "rad", "sin", "cos", "abs", "floor", "sqrt", "random", "min", "max",
        ] {
            math.borrow_mut()
                .set(Value::str(f), Value::Host(Rc::from(format!("math.{f}"))));
        }
        vm.set_global("math", Value::Table(math));
        let table = Table::new_ref();
        for f in ["getn", "insert", "remove"] {
            table
                .borrow_mut()
                .set(Value::str(f), Value::Host(Rc::from(format!("table.{f}"))));
        }
        vm.set_global("table", Value::Table(table));
        vm
    }

    pub fn get_global(&self, name: &str) -> Value {
        self.globals.borrow().get_str(name)
    }

    pub fn set_global(&self, name: &str, v: Value) {
        self.globals.borrow_mut().set(Value::str(name), v);
    }

    /// Runs a chunk's main function.
    pub fn exec_chunk(&mut self, host: &mut dyn Host, proto: Rc<Proto>) -> LuaResult<Vec<Value>> {
        let f = Value::Func(Rc::new(Closure {
            proto,
            upvals: Vec::new(),
        }));
        self.call(host, &f, &[])
    }

    /// Calls any callable value.
    pub fn call(
        &mut self,
        host: &mut dyn Host,
        f: &Value,
        args: &[Value],
    ) -> LuaResult<Vec<Value>> {
        match f {
            Value::Func(c) => {
                let c = c.clone();
                self.call_closure(host, &c, args)
            }
            Value::Host(name) => {
                let name = name.clone();
                self.builtin(host, &name, args)
            }
            Value::Method(o, name) => {
                let name = name.clone();
                host.call_method(self, *o, &name, args)
            }
            v => rt(format!("attempt to call a {} value", v.type_name())),
        }
    }

    fn builtin(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: &[Value],
    ) -> LuaResult<Vec<Value>> {
        let n = |i: usize| args.get(i).and_then(Value::num).unwrap_or(0.0);
        Ok(match name {
            "math.rad" => vec![Value::Num(n(0).to_radians())],
            "math.sin" => vec![Value::Num(n(0).sin())],
            "math.cos" => vec![Value::Num(n(0).cos())],
            "math.abs" => vec![Value::Num(n(0).abs())],
            "math.floor" => vec![Value::Num(n(0).floor())],
            "math.sqrt" => vec![Value::Num(n(0).sqrt())],
            "math.min" => vec![Value::Num(n(0).min(n(1)))],
            "math.max" => vec![Value::Num(n(0).max(n(1)))],
            // Generic `for` over a table (TFORPREP calls the global `next`).
            "next" => match args.first() {
                Some(Value::Table(t)) => match t.borrow().next(args.get(1).unwrap_or(&Value::Nil)) {
                    Some((k, v)) => vec![k, v],
                    None => vec![Value::Nil],
                },
                _ => vec![Value::Nil],
            },
            "table.getn" => match args.first() {
                Some(Value::Table(t)) => vec![Value::Num(t.borrow().len() as f64)],
                _ => vec![Value::Num(0.0)],
            },
            "table.insert" => {
                if let Some(Value::Table(t)) = args.first() {
                    let mut t = t.borrow_mut();
                    if args.len() >= 3 {
                        let pos = n(1) as usize;
                        let len = t.len();
                        for i in (pos..=len).rev() {
                            let v = t.get(&Value::Num(i as f64));
                            t.set(Value::Num((i + 1) as f64), v);
                        }
                        t.set(Value::Num(pos as f64), args[2].clone());
                    } else {
                        let len = t.len();
                        t.set(
                            Value::Num((len + 1) as f64),
                            args.get(1).cloned().unwrap_or_default(),
                        );
                    }
                }
                vec![]
            }
            "table.remove" => {
                if let Some(Value::Table(t)) = args.first() {
                    let mut t = t.borrow_mut();
                    let len = t.len();
                    let pos = if args.len() >= 2 { n(1) as usize } else { len };
                    let out = t.get(&Value::Num(pos as f64));
                    for i in pos..len {
                        let v = t.get(&Value::Num((i + 1) as f64));
                        t.set(Value::Num(i as f64), v);
                    }
                    t.set(Value::Num(len as f64), Value::Nil);
                    vec![out]
                } else {
                    vec![]
                }
            }
            _ => return host.call_host(self, name, args),
        })
    }

    fn close_upvals(&mut self, from: usize) {
        while let Some((idx, _)) = self.open.last() {
            if *idx < from {
                break;
            }
            let (idx, up) = self.open.pop().unwrap();
            let v = self.stack[idx].clone();
            *up.borrow_mut() = Upval::Closed(v);
        }
    }

    fn find_upval(&mut self, idx: usize) -> UpRef {
        if let Some((_, u)) = self.open.iter().find(|(i, _)| *i == idx) {
            return u.clone();
        }
        let u = Rc::new(RefCell::new(Upval::Open(idx)));
        let pos = self.open.partition_point(|(i, _)| *i < idx);
        self.open.insert(pos, (idx, u.clone()));
        u
    }

    fn get_up(&self, u: &UpRef) -> Value {
        match &*u.borrow() {
            Upval::Open(i) => self.stack[*i].clone(),
            Upval::Closed(v) => v.clone(),
        }
    }

    fn set_up(&mut self, u: &UpRef, v: Value) {
        let mut b = u.borrow_mut();
        match &mut *b {
            Upval::Open(i) => self.stack[*i] = v,
            Upval::Closed(c) => *c = v,
        }
    }

    fn index(&mut self, obj: &Value, key: &Value) -> LuaResult<Value> {
        match obj {
            Value::Table(t) => Ok(t.borrow().get(key)),
            Value::Obj(o) => match key {
                Value::Str(s) => Ok(Value::Method(*o, s.clone())),
                _ => rt("index an engine object with a non-string key"),
            },
            v => rt(format!(
                "attempt to index a {} value (key {:?})",
                v.type_name(),
                key
            )),
        }
    }

    fn call_closure(
        &mut self,
        host: &mut dyn Host,
        cl: &Rc<Closure>,
        args: &[Value],
    ) -> LuaResult<Vec<Value>> {
        self.depth += 1;
        if self.depth > 180 {
            self.depth -= 1;
            return rt("stack overflow");
        }
        let base = self.stack.len();
        let p = cl.proto.clone();
        let size = p.max_stack as usize + 2;
        self.stack.resize(base + size, Value::Nil);
        let np = p.params as usize;
        for (i, a) in args.iter().take(np).enumerate() {
            self.stack[base + i] = a.clone();
        }
        if p.vararg {
            let t = Table::new_ref();
            {
                let mut tb = t.borrow_mut();
                let extra = args.iter().skip(np);
                let mut n = 0;
                for (i, a) in extra.enumerate() {
                    tb.set(Value::Num((i + 1) as f64), a.clone());
                    n += 1;
                }
                tb.set(Value::str("n"), Value::Num(n as f64));
            }
            self.stack[base + np] = Value::Table(t);
        }
        let r = self.run(host, cl, base);
        self.close_upvals(base);
        self.stack.truncate(base);
        self.depth -= 1;
        r
    }

    fn ensure(&mut self, top: usize) {
        if self.stack.len() < top {
            self.stack.resize(top, Value::Nil);
        }
    }

    fn arith(&self, o: u32, a: &Value, b: &Value) -> LuaResult<Value> {
        let (Some(x), Some(y)) = (a.num(), b.num()) else {
            return rt(format!(
                "attempt to perform arithmetic on {} and {}",
                a.type_name(),
                b.type_name()
            ));
        };
        Ok(Value::Num(match o {
            12 => x + y,
            13 => x - y,
            14 => x * y,
            15 => x / y,
            _ => x.powf(y),
        }))
    }

    fn less(a: &Value, b: &Value, or_eq: bool) -> LuaResult<bool> {
        match (a, b) {
            (Value::Num(x), Value::Num(y)) => Ok(if or_eq { x <= y } else { x < y }),
            (Value::Str(x), Value::Str(y)) => Ok(if or_eq { x <= y } else { x < y }),
            _ => rt(format!(
                "attempt to compare {} with {}",
                a.type_name(),
                b.type_name()
            )),
        }
    }

    fn run(&mut self, host: &mut dyn Host, cl: &Rc<Closure>, base: usize) -> LuaResult<Vec<Value>> {
        let p = cl.proto.clone();
        let mut pc = 0usize;
        // `top` for multi-value results (absolute stack index), valid right after a CALL with
        // C == 0.
        let mut top = base;
        loop {
            self.steps += 1;
            let Some(&i) = p.code.get(pc) else {
                return Ok(vec![]);
            };
            pc += 1;
            let a = base + arg_a(i);
            macro_rules! rk {
                ($x:expr) => {{
                    let x = $x;
                    if x >= MAXSTACK {
                        p.consts[(x - MAXSTACK) as usize].clone()
                    } else {
                        self.stack[base + x as usize].clone()
                    }
                }};
            }
            match op(i) {
                0 => self.stack[a] = self.stack[base + arg_b(i) as usize].clone(),
                1 => self.stack[a] = p.consts[arg_bx(i)].clone(),
                2 => {
                    self.stack[a] = Value::Bool(arg_b(i) != 0);
                    if arg_c(i) != 0 {
                        pc += 1;
                    }
                }
                3 => {
                    for r in a..=base + arg_b(i) as usize {
                        self.stack[r] = Value::Nil;
                    }
                }
                4 => self.stack[a] = self.get_up(&cl.upvals[arg_b(i) as usize]),
                5 => {
                    let k = &p.consts[arg_bx(i)];
                    self.stack[a] = self.globals.borrow().get(k);
                }
                6 => {
                    let obj = self.stack[base + arg_b(i) as usize].clone();
                    let key = rk!(arg_c(i));
                    self.stack[a] = self.index(&obj, &key).map_err(|e| self.locate(&p, pc, e))?;
                }
                7 => {
                    let k = p.consts[arg_bx(i)].clone();
                    let v = self.stack[a].clone();
                    self.globals.borrow_mut().set(k, v);
                }
                8 => {
                    let v = self.stack[a].clone();
                    self.set_up(&cl.upvals[arg_b(i) as usize], v);
                }
                9 => {
                    let key = rk!(arg_b(i));
                    let v = rk!(arg_c(i));
                    match &self.stack[a] {
                        Value::Table(t) => t.borrow_mut().set(key, v),
                        o => {
                            let msg = format!("attempt to index a {} value", o.type_name());
                            return Err(self.locate(&p, pc, LuaError::Runtime(msg)));
                        }
                    }
                }
                10 => self.stack[a] = Value::Table(Table::new_ref()),
                11 => {
                    let obj = self.stack[base + arg_b(i) as usize].clone();
                    let key = rk!(arg_c(i));
                    self.stack[a + 1] = obj.clone();
                    self.stack[a] = self.index(&obj, &key).map_err(|e| self.locate(&p, pc, e))?;
                }
                o @ 12..=16 => {
                    let x = rk!(arg_b(i));
                    let y = rk!(arg_c(i));
                    self.stack[a] = self.arith(o, &x, &y).map_err(|e| self.locate(&p, pc, e))?;
                }
                17 => {
                    let v = self.stack[base + arg_b(i) as usize].clone();
                    match v.num() {
                        Some(n) => self.stack[a] = Value::Num(-n),
                        None => return Err(self.locate(&p, pc, LuaError::Runtime("unm".into()))),
                    }
                }
                18 => {
                    let v = self.stack[base + arg_b(i) as usize].truthy();
                    self.stack[a] = Value::Bool(!v);
                }
                19 => {
                    let mut s = String::new();
                    for r in arg_b(i)..=arg_c(i) {
                        match self.stack[base + r as usize].to_concat() {
                            Some(x) => s.push_str(&x),
                            None => {
                                return Err(self.locate(
                                    &p,
                                    pc,
                                    LuaError::Runtime("attempt to concatenate".into()),
                                ));
                            }
                        }
                    }
                    self.stack[a] = Value::str(&s);
                }
                20 => pc = (pc as i64 + arg_sbx(i)) as usize,
                o @ 21..=23 => {
                    let x = rk!(arg_b(i));
                    let y = rk!(arg_c(i));
                    let r = match o {
                        21 => x.raw_eq(&y),
                        22 => Self::less(&x, &y, false).map_err(|e| self.locate(&p, pc, e))?,
                        _ => Self::less(&x, &y, true).map_err(|e| self.locate(&p, pc, e))?,
                    };
                    if r != (arg_a(i) != 0) {
                        pc += 1;
                    }
                }
                24 => {
                    let v = self.stack[base + arg_b(i) as usize].clone();
                    if !v.truthy() == (arg_c(i) != 0) {
                        pc += 1;
                    } else {
                        self.stack[a] = v;
                    }
                }
                25 | 26 => {
                    let b = arg_b(i) as usize;
                    let nargs = if b == 0 { top - a - 1 } else { b - 1 };
                    let f = self.stack[a].clone();
                    let args: Vec<Value> = self.stack[a + 1..a + 1 + nargs].to_vec();
                    let res = self
                        .call(host, &f, &args)
                        .map_err(|e| self.locate(&p, pc, e))?;
                    if op(i) == 26 {
                        return Ok(res);
                    }
                    let c = arg_c(i) as usize;
                    if c == 0 {
                        self.ensure(a + res.len() + 1);
                        for (k, v) in res.iter().enumerate() {
                            self.stack[a + k] = v.clone();
                        }
                        top = a + res.len();
                    } else {
                        for k in 0..c - 1 {
                            self.stack[a + k] = res.get(k).cloned().unwrap_or_default();
                        }
                    }
                }
                27 => {
                    let b = arg_b(i) as usize;
                    let n = if b == 0 { top - a } else { b - 1 };
                    return Ok(self.stack[a..a + n].to_vec());
                }
                28 => {
                    let (Some(idx), Some(lim), Some(step)) = (
                        self.stack[a].num(),
                        self.stack[a + 1].num(),
                        self.stack[a + 2].num(),
                    ) else {
                        return Err(self.locate(
                            &p,
                            pc,
                            LuaError::Runtime("'for' needs numbers".into()),
                        ));
                    };
                    let idx = idx + step;
                    if (step > 0.0 && idx <= lim) || (step <= 0.0 && idx >= lim) {
                        pc = (pc as i64 + arg_sbx(i)) as usize;
                        self.stack[a] = Value::Num(idx);
                    }
                }
                29 => {
                    let nvar = arg_c(i) as usize + 1;
                    let f = self.stack[a].clone();
                    let args = vec![self.stack[a + 1].clone(), self.stack[a + 2].clone()];
                    let res = self
                        .call(host, &f, &args)
                        .map_err(|e| self.locate(&p, pc, e))?;
                    self.ensure(a + 2 + nvar + 1);
                    for k in 0..nvar {
                        self.stack[a + 2 + k] = res.get(k).cloned().unwrap_or_default();
                    }
                    if self.stack[a + 2].is_nil() {
                        pc += 1;
                    } else {
                        let j = p.code[pc];
                        pc = (pc as i64 + 1 + arg_sbx(j)) as usize;
                    }
                }
                30 => {
                    if let Value::Table(_) = self.stack[a] {
                        self.stack[a + 1] = self.stack[a].clone();
                        self.stack[a] = Value::Host(Rc::from("next"));
                    }
                    pc = (pc as i64 + arg_sbx(i)) as usize;
                }
                31 | 32 => {
                    let bx = arg_bx(i);
                    let n = if op(i) == 31 {
                        bx % FIELDS_PER_FLUSH + 1
                    } else {
                        top - a - 1
                    };
                    let start = bx - bx % FIELDS_PER_FLUSH;
                    if let Value::Table(t) = self.stack[a].clone() {
                        let mut t = t.borrow_mut();
                        for k in 1..=n {
                            t.set(Value::Num((start + k) as f64), self.stack[a + k].clone());
                        }
                    }
                }
                33 => self.close_upvals(a),
                34 => {
                    let np = p.protos[arg_bx(i)].clone();
                    let mut ups = Vec::with_capacity(np.nups as usize);
                    for _ in 0..np.nups {
                        let j = p.code[pc];
                        pc += 1;
                        if op(j) == 4 {
                            ups.push(cl.upvals[arg_b(j) as usize].clone());
                        } else {
                            ups.push(self.find_upval(base + arg_b(j) as usize));
                        }
                    }
                    self.stack[a] = Value::Func(Rc::new(Closure {
                        proto: np,
                        upvals: ups,
                    }));
                }
                o => return rt(format!("unknown opcode {o}")),
            }
        }
    }

    fn locate(&self, p: &Proto, pc: usize, e: LuaError) -> LuaError {
        match e {
            LuaError::Runtime(m) if !m.starts_with('[') => {
                let line = p.lines.get(pc.saturating_sub(1)).copied().unwrap_or(0);
                LuaError::Runtime(format!("[{}:{}] {}", p.source, line, m))
            }
            e => e,
        }
    }
}
