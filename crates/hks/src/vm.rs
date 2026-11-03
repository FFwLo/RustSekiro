//! The HavokScript interpreter.
//!
//! A register machine over one shared value stack, following Lua 5.1 semantics with the HKS
//! opcode set and 32-bit float numbers. Only the opcodes the shipped scripts use are executed;
//! the rest fail loudly with [`VmError::Unsupported`].

use crate::host::{CommandId, ENGINE_FUNCTIONS, Host};
use crate::table::{Table, TableRef};
use crate::value::{Function, LStr, Value};
use sekiro_formats::hks::{self, Constant, HksFile, Instruction, Op, RK_CONSTANT};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("{message}\n{traceback}")]
    Runtime { message: String, traceback: String },
    #[error("unsupported opcode {op} in {function} at pc {pc}")]
    Unsupported {
        op: &'static str,
        function: String,
        pc: usize,
    },
    #[error("no global function {0:?}")]
    NoSuchFunction(String),
    #[error(transparent)]
    Format(#[from] sekiro_formats::Error),
}

pub type VmResult<T> = Result<T, VmError>;

/// A prototype converted for execution: constants become values, children are shared.
pub struct FnProto {
    pub name: String,
    pub chunk: Rc<str>,
    pub num_params: usize,
    pub is_vararg: bool,
    pub max_stack: usize,
    pub num_upvalues: usize,
    pub code: Vec<Instruction>,
    pub k: Vec<Value>,
    pub protos: Vec<Rc<FnProto>>,
    pub lines: Vec<i32>,
}

impl FnProto {
    pub fn from_proto(p: &hks::Proto, chunk: &Rc<str>) -> Rc<FnProto> {
        Rc::new(FnProto {
            name: p.name(),
            chunk: chunk.clone(),
            num_params: p.num_params as usize,
            is_vararg: p.vararg != 0,
            max_stack: p.max_stack as usize,
            num_upvalues: p.num_upvalues as usize,
            code: p.code.clone(),
            k: p.constants
                .iter()
                .map(|c| match c {
                    Constant::Nil => Value::Nil,
                    Constant::Bool(b) => Value::Bool(*b),
                    Constant::Number(n) => Value::Number(*n),
                    Constant::String(s) => Value::bytes(s),
                })
                .collect(),
            protos: p
                .protos
                .iter()
                .map(|c| FnProto::from_proto(c, chunk))
                .collect(),
            lines: p
                .debug
                .as_ref()
                .map(|d| d.line_info.clone())
                .unwrap_or_default(),
        })
    }
}

/// An upvalue: a live stack slot until its frame returns, then a closed-over value.
pub enum Upval {
    Open(usize),
    Closed(Value),
}

pub struct LuaClosure {
    pub proto: Rc<FnProto>,
    pub upvals: Vec<Rc<RefCell<Upval>>>,
}

/// Functions implemented in Rust: the small standard library subset the scripts use, plus
/// engine functions forwarded to the [`Host`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Builtin {
    Engine(Rc<str>),
    Print,
    CollectGarbage,
    Type,
    ToString,
    ToNumber,
    Pairs,
    IPairs,
    IPairsIter,
    Next,
    Select,
    Unpack,
    SetMetatable,
    GetMetatable,
    RawGet,
    RawSet,
    Assert,
    Error,
    MathAbs,
    MathRandom,
    MathFloor,
    MathCeil,
    MathMax,
    MathMin,
    MathSqrt,
    MathSin,
    MathCos,
    MathAtan2,
    MathMod,
    TableInsert,
    TableRemove,
    StringFormat,
}

const MAX_DEPTH: usize = 180;
const FIELDS_PER_FLUSH: usize = 50;

struct FrameInfo {
    name: String,
    chunk: Rc<str>,
    pc: usize,
    line: i32,
}

pub struct Vm {
    stack: Vec<Value>,
    globals: TableRef,
    open_upvals: Vec<Rc<RefCell<Upval>>>,
    frames: Vec<FrameInfo>,
    rng: u64,
    /// Instructions executed since creation; useful for profiling and runaway detection.
    pub instructions: u64,
    /// Lines passed to `print`.
    pub printed: Vec<String>,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Vm {
        let mut vm = Vm {
            stack: Vec::with_capacity(1024),
            globals: Table::new_ref(),
            open_upvals: Vec::new(),
            frames: Vec::new(),
            rng: 0x2545_f491_4f6c_dd1d,
            instructions: 0,
            printed: Vec::new(),
        };
        vm.open_stdlib();
        vm
    }

    pub fn globals(&self) -> &TableRef {
        &self.globals
    }

    pub fn get_global(&self, name: &str) -> Value {
        self.globals.borrow().get_str(name)
    }

    pub fn set_global(&mut self, name: &str, value: Value) {
        self.globals
            .borrow_mut()
            .set(Value::str(name), value)
            .expect("string key");
    }

    /// Seeds `math.random`. The game seeds its own generator; tests want a fixed sequence.
    pub fn seed(&mut self, seed: u64) {
        self.rng = seed | 1;
    }

    fn open_stdlib(&mut self) {
        let b = |x| Value::Function(Function::Builtin(x));
        self.set_global("_G", Value::Table(self.globals.clone()));
        for (name, f) in [
            ("print", Builtin::Print),
            ("collectgarbage", Builtin::CollectGarbage),
            ("type", Builtin::Type),
            ("tostring", Builtin::ToString),
            ("tonumber", Builtin::ToNumber),
            ("pairs", Builtin::Pairs),
            ("ipairs", Builtin::IPairs),
            ("next", Builtin::Next),
            ("select", Builtin::Select),
            ("unpack", Builtin::Unpack),
            ("setmetatable", Builtin::SetMetatable),
            ("getmetatable", Builtin::GetMetatable),
            ("rawget", Builtin::RawGet),
            ("rawset", Builtin::RawSet),
            ("assert", Builtin::Assert),
            ("error", Builtin::Error),
        ] {
            self.set_global(name, b(f));
        }
        let lib = |entries: &[(&str, Builtin)]| {
            let t = Table::new_ref();
            for (name, f) in entries {
                t.borrow_mut()
                    .set(Value::str(name), b(f.clone()))
                    .expect("string key");
            }
            Value::Table(t)
        };
        let math = lib(&[
            ("abs", Builtin::MathAbs),
            ("random", Builtin::MathRandom),
            ("floor", Builtin::MathFloor),
            ("ceil", Builtin::MathCeil),
            ("max", Builtin::MathMax),
            ("min", Builtin::MathMin),
            ("sqrt", Builtin::MathSqrt),
            ("sin", Builtin::MathSin),
            ("cos", Builtin::MathCos),
            ("atan2", Builtin::MathAtan2),
            ("mod", Builtin::MathMod),
            ("fmod", Builtin::MathMod),
        ]);
        if let Value::Table(t) = &math {
            let mut t = t.borrow_mut();
            let _ = t.set(Value::str("pi"), Value::Number(std::f32::consts::PI));
            let _ = t.set(Value::str("huge"), Value::Number(f32::INFINITY));
        }
        self.set_global("math", math);
        self.set_global(
            "table",
            lib(&[
                ("insert", Builtin::TableInsert),
                ("remove", Builtin::TableRemove),
            ]),
        );
        self.set_global("string", lib(&[("format", Builtin::StringFormat)]));
        for name in ENGINE_FUNCTIONS {
            self.set_global(name, b(Builtin::Engine(Rc::from(*name))));
        }
    }

    /// Exposes another engine function to the scripts, forwarded to [`Host::call`].
    pub fn register_engine_function(&mut self, name: &str) {
        self.set_global(
            name,
            Value::Function(Function::Builtin(Builtin::Engine(Rc::from(name)))),
        );
    }

    /// Runs a compiled chunk's main function, which defines its globals.
    pub fn load(&mut self, host: &mut dyn Host, chunk_name: &str, data: &[u8]) -> VmResult<()> {
        let file: HksFile = hks::parse(data)?;
        let proto = FnProto::from_proto(&file.main, &Rc::from(chunk_name));
        let closure = Rc::new(LuaClosure {
            proto,
            upvals: Vec::new(),
        });
        self.call_value(host, &Value::Function(Function::Lua(closure)), &[])?;
        Ok(())
    }

    /// Calls the global function `name` if it exists as a real function.
    ///
    /// The player scripts install a `_G` metatable that returns a no-op `dummy` for unknown
    /// names, so a missing callback is not an error in the game either.
    pub fn call_global(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: &[Value],
    ) -> VmResult<Vec<Value>> {
        let f = self.get_global(name);
        if !matches!(f, Value::Function(_)) {
            return Err(VmError::NoSuchFunction(name.to_owned()));
        }
        self.call_value(host, &f, args)
    }

    pub fn has_function(&self, name: &str) -> bool {
        matches!(self.get_global(name), Value::Function(_))
    }

    fn error(&self, message: impl Into<String>) -> VmError {
        let mut traceback = String::new();
        for f in self.frames.iter().rev() {
            traceback.push_str(&format!(
                "  in {} ({}:{}, pc {})\n",
                f.name, f.chunk, f.line, f.pc
            ));
        }
        VmError::Runtime {
            message: message.into(),
            traceback,
        }
    }

    pub fn call_value(
        &mut self,
        host: &mut dyn Host,
        f: &Value,
        args: &[Value],
    ) -> VmResult<Vec<Value>> {
        match f {
            Value::Function(Function::Lua(c)) => self.call_lua(host, c.clone(), args),
            Value::Function(Function::Builtin(b)) => self.call_builtin(host, b, args),
            Value::Table(t) => {
                let call = t
                    .borrow()
                    .metatable
                    .as_ref()
                    .map(|m| m.borrow().get_str("__call"))
                    .unwrap_or_default();
                if call.is_nil() {
                    return Err(self.error("attempt to call a table value"));
                }
                let mut with_self = vec![f.clone()];
                with_self.extend_from_slice(args);
                self.call_value(host, &call, &with_self)
            }
            other => Err(self.error(format!("attempt to call a {} value", other.type_name()))),
        }
    }

    fn call_lua(
        &mut self,
        host: &mut dyn Host,
        closure: Rc<LuaClosure>,
        args: &[Value],
    ) -> VmResult<Vec<Value>> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(self.error("stack overflow"));
        }
        let proto = closure.proto.clone();
        let base = self.stack.len();
        self.stack.resize(base + proto.max_stack.max(1), Value::Nil);
        for (i, a) in args.iter().take(proto.num_params).enumerate() {
            self.stack[base + i] = a.clone();
        }
        let varargs = if proto.is_vararg && args.len() > proto.num_params {
            args[proto.num_params..].to_vec()
        } else {
            Vec::new()
        };
        self.frames.push(FrameInfo {
            name: proto.name.clone(),
            chunk: proto.chunk.clone(),
            pc: 0,
            line: 0,
        });
        let result = self.execute(host, &closure, base, varargs);
        self.close_upvals(base);
        self.stack.truncate(base);
        self.frames.pop();
        result
    }

    fn close_upvals(&mut self, level: usize) {
        let stack = &self.stack;
        self.open_upvals.retain(|u| {
            let mut u = u.borrow_mut();
            if let Upval::Open(idx) = *u
                && idx >= level
            {
                *u = Upval::Closed(stack[idx].clone());
                return false;
            }
            true
        });
    }

    fn find_upval(&mut self, idx: usize) -> Rc<RefCell<Upval>> {
        for u in &self.open_upvals {
            if matches!(*u.borrow(), Upval::Open(i) if i == idx) {
                return u.clone();
            }
        }
        let u = Rc::new(RefCell::new(Upval::Open(idx)));
        self.open_upvals.push(u.clone());
        u
    }

    /// Table read honouring `__index` (function or table).
    pub fn index(&mut self, host: &mut dyn Host, obj: &Value, key: &Value) -> VmResult<Value> {
        let mut obj = obj.clone();
        for _ in 0..100 {
            let Value::Table(t) = &obj else {
                return Err(self.error(format!(
                    "attempt to index a {} value (key {key:?})",
                    obj.type_name()
                )));
            };
            let (v, handler) = {
                let t = t.borrow();
                let v = t.get(key);
                let handler = if v.is_nil() {
                    t.metatable
                        .as_ref()
                        .map(|m| m.borrow().get_str("__index"))
                        .unwrap_or_default()
                } else {
                    Value::Nil
                };
                (v, handler)
            };
            match handler {
                Value::Nil => return Ok(v),
                Value::Function(_) => {
                    let r = self.call_value(host, &handler, &[obj.clone(), key.clone()])?;
                    return Ok(r.into_iter().next().unwrap_or_default());
                }
                other => obj = other,
            }
        }
        Err(self.error("'__index' chain too long"))
    }

    /// Table write. `__newindex` is not used by the shipped scripts, so writes are raw.
    pub fn set_index(&mut self, obj: &Value, key: Value, value: Value) -> VmResult<()> {
        let Value::Table(t) = obj else {
            return Err(self.error(format!(
                "attempt to index a {} value (key {key:?})",
                obj.type_name()
            )));
        };
        t.borrow_mut().set(key, value).map_err(|m| self.error(m))
    }

    fn arith(&self, op: Op, a: &Value, b: &Value) -> VmResult<Value> {
        let (Some(x), Some(y)) = (a.as_number(), b.as_number()) else {
            let bad = if a.as_number().is_none() { a } else { b };
            return Err(self.error(format!(
                "attempt to perform arithmetic on a {} value",
                bad.type_name()
            )));
        };
        Ok(Value::Number(match op {
            Op::Add | Op::AddBk => x + y,
            Op::Sub | Op::SubBk => x - y,
            Op::Mul | Op::MulBk => x * y,
            Op::Div | Op::DivBk => x / y,
            Op::Mod | Op::ModBk => x - (x / y).floor() * y,
            _ => x.powf(y),
        }))
    }

    fn less_than(&self, a: &Value, b: &Value, or_equal: bool) -> VmResult<bool> {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => Ok(if or_equal { x <= y } else { x < y }),
            (Value::Str(x), Value::Str(y)) => Ok(if or_equal { x <= y } else { x < y }),
            _ => Err(self.error(format!(
                "attempt to compare {} with {}",
                a.type_name(),
                b.type_name()
            ))),
        }
    }

    fn execute(
        &mut self,
        host: &mut dyn Host,
        closure: &Rc<LuaClosure>,
        base: usize,
        varargs: Vec<Value>,
    ) -> VmResult<Vec<Value>> {
        let proto = closure.proto.clone();
        let code = &proto.code;
        let k = &proto.k;
        let mut pc = 0usize;
        // Frame-relative index one past the last valid register after a multi-result op.
        let mut top = proto.max_stack;

        macro_rules! r {
            ($i:expr) => {
                self.stack[base + ($i) as usize]
            };
        }
        macro_rules! rk {
            ($v:expr) => {{
                let v = $v;
                if v & RK_CONSTANT != 0 {
                    k[(v & 0xFF) as usize].clone()
                } else {
                    r!(v).clone()
                }
            }};
        }

        loop {
            let Some(&ins) = code.get(pc) else {
                return Err(self.error("fell off the end of the function"));
            };
            if let Some(f) = self.frames.last_mut() {
                f.pc = pc;
                f.line = proto.lines.get(pc).copied().unwrap_or(0);
            }
            self.instructions += 1;
            pc += 1;
            let a = ins.a() as usize;
            let op = ins
                .op()
                .ok_or_else(|| self.error(format!("bad opcode {}", ins.raw_op())))?;
            match op {
                Op::Data => {}
                Op::Move => r!(a) = r!(ins.b()).clone(),
                Op::LoadK => r!(a) = k[ins.bx() as usize].clone(),
                Op::LoadBool => {
                    r!(a) = Value::Bool(ins.b() != 0);
                    if ins.c() != 0 {
                        pc += 1;
                    }
                }
                Op::LoadNil => {
                    for i in a..=ins.b() as usize {
                        r!(i) = Value::Nil;
                    }
                }
                Op::GetUpval => {
                    let u = closure.upvals[ins.b() as usize].clone();
                    let v = match &*u.borrow() {
                        Upval::Open(i) => self.stack[*i].clone(),
                        Upval::Closed(v) => v.clone(),
                    };
                    r!(a) = v;
                }
                Op::SetUpval | Op::SetUpvalR1 => {
                    let v = r!(a).clone();
                    let u = closure.upvals[ins.b() as usize].clone();
                    let mut u = u.borrow_mut();
                    match &mut *u {
                        Upval::Open(i) => self.stack[*i] = v,
                        Upval::Closed(c) => *c = v,
                    }
                }
                Op::GetGlobal | Op::GetGlobalMem => {
                    let key = k[ins.bx() as usize].clone();
                    let g = Value::Table(self.globals.clone());
                    r!(a) = self.index(host, &g, &key)?;
                }
                Op::SetGlobal => {
                    let key = k[ins.bx() as usize].clone();
                    let v = r!(a).clone();
                    self.globals
                        .borrow_mut()
                        .set(key, v)
                        .map_err(|m| self.error(m))?;
                }
                Op::GetField | Op::GetFieldR1 => {
                    let obj = r!(ins.b()).clone();
                    let key = k[ins.c() as usize].clone();
                    r!(a) = self.index(host, &obj, &key)?;
                }
                Op::GetTable | Op::GetTableS => {
                    let obj = r!(ins.b()).clone();
                    let key = rk!(ins.c());
                    r!(a) = self.index(host, &obj, &key)?;
                }
                Op::SetField | Op::SetFieldR1 | Op::SetTableSBk => {
                    let obj = r!(a).clone();
                    let key = k[ins.b() as usize].clone();
                    let v = rk!(ins.c());
                    self.set_index(&obj, key, v)?;
                }
                Op::SetTable | Op::SetTableS => {
                    let obj = r!(a).clone();
                    let key = r!(ins.b()).clone();
                    let v = rk!(ins.c());
                    self.set_index(&obj, key, v)?;
                }
                Op::NewTable => r!(a) = Value::Table(Table::new_ref()),
                Op::SelfOp => {
                    let obj = r!(ins.b()).clone();
                    let key = rk!(ins.c());
                    r!(a + 1) = obj.clone();
                    r!(a) = self.index(host, &obj, &key)?;
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                    let x = r!(ins.b()).clone();
                    let y = rk!(ins.c());
                    r!(a) = self.arith(op, &x, &y)?;
                }
                Op::AddBk | Op::SubBk | Op::MulBk | Op::DivBk | Op::ModBk | Op::PowBk => {
                    let x = k[ins.b() as usize].clone();
                    let y = r!(ins.c()).clone();
                    r!(a) = self.arith(op, &x, &y)?;
                }
                Op::Unm => {
                    let x = r!(ins.b()).clone();
                    r!(a) = self.arith(Op::Sub, &Value::Number(0.0), &x)?;
                }
                Op::Not | Op::NotR1 => r!(a) = Value::Bool(!r!(ins.b()).truthy()),
                Op::Len => {
                    let v = match &r!(ins.b()) {
                        Value::Table(t) => Value::Number(t.borrow().len() as f32),
                        Value::Str(s) => Value::Number(s.len() as f32),
                        other => {
                            return Err(self.error(format!(
                                "attempt to get length of a {} value",
                                other.type_name()
                            )));
                        }
                    };
                    r!(a) = v;
                }
                Op::Concat => {
                    let mut out = Vec::new();
                    for i in ins.b()..=ins.c() {
                        match &r!(i) {
                            Value::Str(s) => out.extend_from_slice(s),
                            Value::Number(n) => {
                                out.extend_from_slice(hks::format_number(*n).as_bytes())
                            }
                            other => {
                                return Err(self.error(format!(
                                    "attempt to concatenate a {} value",
                                    other.type_name()
                                )));
                            }
                        }
                    }
                    r!(a) = Value::Str(LStr::from(out));
                }
                Op::Jmp => pc = jump(pc, ins),
                Op::Eq => {
                    let eq = r!(ins.b()).raw_eq(&rk!(ins.c()));
                    if eq != (a != 0) {
                        pc += 1;
                    }
                }
                Op::EqBk => {
                    let eq = k[ins.b() as usize].raw_eq(&r!(ins.c()));
                    if eq != (a != 0) {
                        pc += 1;
                    }
                }
                Op::Lt | Op::Le => {
                    let x = r!(ins.b()).clone();
                    let y = rk!(ins.c());
                    if self.less_than(&x, &y, op == Op::Le)? != (a != 0) {
                        pc += 1;
                    }
                }
                Op::LtBk | Op::LeBk => {
                    let x = k[ins.b() as usize].clone();
                    let y = r!(ins.c()).clone();
                    if self.less_than(&x, &y, op == Op::LeBk)? != (a != 0) {
                        pc += 1;
                    }
                }
                Op::Test | Op::TestR1 => {
                    if r!(a).truthy() != (ins.c() != 0) {
                        pc += 1;
                    }
                }
                Op::TestSet => {
                    let v = r!(ins.b()).clone();
                    if v.truthy() == (ins.c() != 0) {
                        r!(a) = v;
                    } else {
                        pc += 1;
                    }
                }
                Op::Call | Op::CallI | Op::CallIR1 | Op::CallC | Op::CallM => {
                    let (b, c) = (ins.b() as usize, ins.c() as usize);
                    let nargs = if b == 0 { top - a - 1 } else { b - 1 };
                    let f = r!(a).clone();
                    let args: Vec<Value> = self.stack[base + a + 1..base + a + 1 + nargs].to_vec();
                    let results = self.call_value(host, &f, &args)?;
                    if c == 0 {
                        self.place(base, a, &results);
                        top = a + results.len();
                    } else {
                        for i in 0..c - 1 {
                            r!(a + i) = results.get(i).cloned().unwrap_or_default();
                        }
                    }
                }
                Op::TailCall | Op::TailCallI | Op::TailCallIR1 | Op::TailCallC | Op::TailCallM => {
                    let b = ins.b() as usize;
                    let nargs = if b == 0 { top - a - 1 } else { b - 1 };
                    let f = r!(a).clone();
                    let args: Vec<Value> = self.stack[base + a + 1..base + a + 1 + nargs].to_vec();
                    return self.call_value(host, &f, &args);
                }
                Op::Return => {
                    let b = ins.b() as usize;
                    let end = if b == 0 { top } else { a + b - 1 };
                    return Ok(self.stack[base + a..base + end].to_vec());
                }
                Op::VarArg => {
                    let b = ins.b() as usize;
                    if b == 0 {
                        self.place(base, a, &varargs);
                        top = a + varargs.len();
                    } else {
                        for i in 0..b - 1 {
                            r!(a + i) = varargs.get(i).cloned().unwrap_or_default();
                        }
                    }
                }
                Op::SetList => {
                    let (b, c) = (ins.b() as usize, ins.c() as usize);
                    if c == 0 {
                        return Err(self.error("SETLIST with extended block number"));
                    }
                    let n = if b == 0 { top - a - 1 } else { b };
                    let Value::Table(t) = r!(a).clone() else {
                        return Err(self.error("SETLIST on a non-table"));
                    };
                    let mut t = t.borrow_mut();
                    for j in 1..=n {
                        let idx = (c - 1) * FIELDS_PER_FLUSH + j;
                        t.set(Value::Number(idx as f32), r!(a + j).clone())
                            .map_err(|m| self.error(m))?;
                    }
                }
                Op::ForPrep => {
                    let (Some(init), Some(step)) = (r!(a).as_number(), r!(a + 2).as_number())
                    else {
                        return Err(self.error("'for' initial value and step must be numbers"));
                    };
                    if r!(a + 1).as_number().is_none() {
                        return Err(self.error("'for' limit must be a number"));
                    }
                    r!(a) = Value::Number(init - step);
                    pc = jump(pc, ins);
                }
                Op::ForLoop => {
                    let step = r!(a + 2).as_number().unwrap_or(1.0);
                    let limit = r!(a + 1).as_number().unwrap_or(0.0);
                    let idx = r!(a).as_number().unwrap_or(0.0) + step;
                    let go_on = if step > 0.0 {
                        idx <= limit
                    } else {
                        limit <= idx
                    };
                    if go_on {
                        pc = jump(pc, ins);
                        r!(a) = Value::Number(idx);
                        r!(a + 3) = Value::Number(idx);
                    }
                }
                Op::TForLoop => {
                    let c = ins.c() as usize;
                    let f = r!(a).clone();
                    let args = [r!(a + 1).clone(), r!(a + 2).clone()];
                    let results = self.call_value(host, &f, &args)?;
                    for i in 0..c {
                        r!(a + 3 + i) = results.get(i).cloned().unwrap_or_default();
                    }
                    if r!(a + 3).is_nil() {
                        pc += 1;
                    } else {
                        r!(a + 2) = r!(a + 3).clone();
                    }
                }
                Op::Close => self.close_upvals(base + a),
                Op::Closure => {
                    let child = proto.protos[ins.bx() as usize].clone();
                    let mut upvals = Vec::with_capacity(child.num_upvalues);
                    for _ in 0..child.num_upvalues {
                        let bind = code[pc];
                        pc += 1;
                        match bind.a() {
                            1 => upvals.push(self.find_upval(base + bind.bx() as usize)),
                            2 => upvals.push(closure.upvals[bind.bx() as usize].clone()),
                            other => {
                                return Err(
                                    self.error(format!("unknown upvalue binding kind {other}"))
                                );
                            }
                        }
                    }
                    r!(a) = Value::Function(Function::Lua(Rc::new(LuaClosure {
                        proto: child,
                        upvals,
                    })));
                }
                _ => {
                    return Err(VmError::Unsupported {
                        op: op.name(),
                        function: proto.name.clone(),
                        pc: pc - 1,
                    });
                }
            }
        }
    }

    /// Writes `values` to registers `a..`, growing the stack past the frame if needed.
    fn place(&mut self, base: usize, a: usize, values: &[Value]) {
        let end = base + a + values.len();
        if self.stack.len() < end {
            self.stack.resize(end, Value::Nil);
        }
        self.stack[base + a..end].clone_from_slice(values);
    }

    fn random(&mut self) -> f32 {
        // xorshift64*; deterministic and seedable.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn call_builtin(
        &mut self,
        host: &mut dyn Host,
        f: &Builtin,
        args: &[Value],
    ) -> VmResult<Vec<Value>> {
        let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
        let num = |vm: &Vm, i: usize| {
            arg(i)
                .as_number()
                .ok_or_else(|| vm.error(format!("bad argument #{} (number expected)", i + 1)))
        };
        let table = |vm: &Vm, i: usize| match arg(i) {
            Value::Table(t) => Ok(t),
            other => Err(vm.error(format!(
                "bad argument #{} (table expected, got {})",
                i + 1,
                other.type_name()
            ))),
        };
        let one = |v: Value| Ok(vec![v]);
        match f {
            Builtin::Engine(name) => {
                let id = |v: &Value| match v {
                    Value::Number(n) => Some(CommandId::Number(*n as i32)),
                    _ => None,
                };
                match &**name {
                    "env" | "act" => {
                        let first = arg(0);
                        let text;
                        let cid = match id(&first) {
                            Some(c) => c,
                            None => {
                                text = first.to_display();
                                CommandId::Name(&text)
                            }
                        };
                        let rest = args.get(1..).unwrap_or(&[]);
                        let v = if &**name == "env" {
                            host.env(cid, rest)
                        } else {
                            host.act(cid, rest)
                        };
                        one(v)
                    }
                    other => Ok(host.call(other, args)),
                }
            }
            Builtin::Print => {
                let line: Vec<String> = args.iter().map(Value::to_display).collect();
                self.printed.push(line.join("\t"));
                Ok(Vec::new())
            }
            Builtin::CollectGarbage => one(Value::Number(0.0)),
            Builtin::Type => one(Value::str(arg(0).type_name())),
            Builtin::ToString => one(Value::str(&arg(0).to_display())),
            Builtin::ToNumber => one(arg(0).as_number().map(Value::Number).unwrap_or_default()),
            Builtin::Pairs => Ok(vec![
                Value::Function(Function::Builtin(Builtin::Next)),
                Value::Table(table(self, 0)?),
                Value::Nil,
            ]),
            Builtin::Next => {
                let t = table(self, 0)?;
                let next = t.borrow().next(&arg(1)).map_err(|m| self.error(m))?;
                Ok(match next {
                    Some((k, v)) => vec![k, v],
                    None => vec![Value::Nil],
                })
            }
            Builtin::IPairs => Ok(vec![
                Value::Function(Function::Builtin(Builtin::IPairsIter)),
                Value::Table(table(self, 0)?),
                Value::Number(0.0),
            ]),
            Builtin::IPairsIter => {
                let t = table(self, 0)?;
                let i = num(self, 1)? as usize + 1;
                let v = t.borrow().get_int(i);
                Ok(if v.is_nil() {
                    vec![Value::Nil]
                } else {
                    vec![Value::Number(i as f32), v]
                })
            }
            Builtin::Select => match arg(0) {
                Value::Str(s) if &*s == b"#" => one(Value::Number((args.len() - 1) as f32)),
                _ => {
                    let n = num(self, 0)? as usize;
                    Ok(args.get(n..).map(<[Value]>::to_vec).unwrap_or_default())
                }
            },
            Builtin::Unpack => {
                let t = table(self, 0)?;
                let t = t.borrow();
                Ok((1..=t.len()).map(|i| t.get_int(i)).collect())
            }
            Builtin::SetMetatable => {
                let t = table(self, 0)?;
                t.borrow_mut().metatable = match arg(1) {
                    Value::Table(m) => Some(m),
                    _ => None,
                };
                one(Value::Table(t))
            }
            Builtin::GetMetatable => one(match arg(0) {
                Value::Table(t) => t
                    .borrow()
                    .metatable
                    .clone()
                    .map(Value::Table)
                    .unwrap_or_default(),
                _ => Value::Nil,
            }),
            Builtin::RawGet => {
                let t = table(self, 0)?;
                one(t.borrow().get(&arg(1)))
            }
            Builtin::RawSet => {
                let t = table(self, 0)?;
                t.borrow_mut()
                    .set(arg(1), arg(2))
                    .map_err(|m| self.error(m))?;
                one(Value::Table(t))
            }
            Builtin::Assert => {
                if arg(0).truthy() {
                    Ok(args.to_vec())
                } else {
                    Err(self.error(format!("assertion failed: {}", arg(1).to_display())))
                }
            }
            Builtin::Error => Err(self.error(arg(0).to_display())),
            Builtin::MathAbs => one(Value::Number(num(self, 0)?.abs())),
            Builtin::MathFloor => one(Value::Number(num(self, 0)?.floor())),
            Builtin::MathCeil => one(Value::Number(num(self, 0)?.ceil())),
            Builtin::MathSqrt => one(Value::Number(num(self, 0)?.sqrt())),
            Builtin::MathSin => one(Value::Number(num(self, 0)?.sin())),
            Builtin::MathCos => one(Value::Number(num(self, 0)?.cos())),
            Builtin::MathAtan2 => one(Value::Number(num(self, 0)?.atan2(num(self, 1)?))),
            Builtin::MathMod => {
                let (x, y) = (num(self, 0)?, num(self, 1)?);
                one(Value::Number(x % y))
            }
            Builtin::MathMax | Builtin::MathMin => {
                let mut best = num(self, 0)?;
                for i in 1..args.len() {
                    let v = num(self, i)?;
                    if (*f == Builtin::MathMax && v > best) || (*f == Builtin::MathMin && v < best)
                    {
                        best = v;
                    }
                }
                one(Value::Number(best))
            }
            Builtin::MathRandom => {
                let r = self.random();
                one(Value::Number(match args.len() {
                    0 => r,
                    1 => (r * num(self, 0)?).floor() + 1.0,
                    _ => {
                        let (lo, hi) = (num(self, 0)?, num(self, 1)?);
                        lo + (r * (hi - lo + 1.0)).floor()
                    }
                }))
            }
            Builtin::TableInsert => {
                let t = table(self, 0)?;
                let mut t = t.borrow_mut();
                let n = t.len();
                let res = if args.len() >= 3 {
                    let pos = num(self, 1)? as usize;
                    let mut res = Ok(());
                    for i in (pos..=n).rev() {
                        let v = t.get_int(i);
                        res = res.and(t.set(Value::Number((i + 1) as f32), v));
                    }
                    res.and(t.set(Value::Number(pos as f32), arg(2)))
                } else {
                    t.set(Value::Number((n + 1) as f32), arg(1))
                };
                res.map_err(|m| self.error(m))?;
                Ok(Vec::new())
            }
            Builtin::TableRemove => {
                let t = table(self, 0)?;
                let mut t = t.borrow_mut();
                let n = t.len();
                if n == 0 {
                    return one(Value::Nil);
                }
                let pos = if args.len() >= 2 {
                    num(self, 1)? as usize
                } else {
                    n
                };
                let removed = t.get_int(pos);
                let mut res = Ok(());
                for i in pos..n {
                    let v = t.get_int(i + 1);
                    res = res.and(t.set(Value::Number(i as f32), v));
                }
                res.and(t.set(Value::Number(n as f32), Value::Nil))
                    .map_err(|m| self.error(m))?;
                one(removed)
            }
            Builtin::StringFormat => {
                // Only used by debug printing; join the arguments rather than emulate printf.
                let parts: Vec<String> = args.iter().map(Value::to_display).collect();
                one(Value::str(&parts.join(" ")))
            }
        }
    }
}

/// Target of a jump-style instruction whose `pc` has already advanced past it.
fn jump(pc: usize, ins: Instruction) -> usize {
    (pc as i64 + ins.sbx() as i64) as usize
}
