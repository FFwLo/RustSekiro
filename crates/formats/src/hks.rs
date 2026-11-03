//! HavokScript (HKS) compiled bytecode, the format of `action/script/*.hks`.
//!
//! HKS is Havok's Lua 5.1 derivative. Sekiro ships it compiled, big-endian, with 32-bit floats
//! as the only number type. The layout below was learned from public community work (horkrux's
//! hksdisasm notes as used by katalash's DSLuaDecompiler) and checked against the installed
//! files; the reader itself is original.
//!
//! File layout (all integers big-endian):
//! - header: `1B 'Lua'`, version 0x51, format 0x0E, endianness, int/size_t/instruction/number
//!   sizes, number-is-integral flag, build flags, one reserved byte;
//! - a type table: `u32 count`, then `u32 id, u32 len, name\0` per type (`TNIL`, `TSTRUCT`, ...);
//! - the main function prototype, with child prototypes nested depth first.
//!
//! A prototype is `u32 upvalues, u32 params, u8 vararg, u32 max_stack, u32 unknown, u32 code_len`,
//! padding to a 4-byte file offset, the code, the constants, a debug block and the children.

use crate::reader::{Error, Reader, Result, bail};
use std::fmt::{self, Write as _};

/// The fixed part of the file header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub format: u8,
    pub little_endian: bool,
    pub int_size: u8,
    pub size_t_size: u8,
    pub instruction_size: u8,
    pub number_size: u8,
    pub number_integral: bool,
    pub build_flags: u8,
    pub reserved: u8,
}

/// One entry of the runtime type table the compiler records in the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEntry {
    pub id: u32,
    pub name: String,
}

/// A constant from a prototype's constant table. HKS numbers are 32-bit floats.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Nil,
    Bool(bool),
    Number(f32),
    /// Raw bytes; the scripts mix ASCII identifiers with UTF-8 Japanese command names.
    String(Vec<u8>),
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constant::Nil => f.write_str("nil"),
            Constant::Bool(b) => write!(f, "{b}"),
            Constant::Number(n) => f.write_str(&format_number(*n)),
            Constant::String(s) => write!(f, "{:?}", String::from_utf8_lossy(s)),
        }
    }
}

/// Formats a number the way the scripts read best: integers without a fraction.
pub fn format_number(n: f32) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1.0e9 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// A named local variable's live range, in instruction indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVar {
    pub name: Option<Vec<u8>>,
    pub start_pc: i32,
    pub end_pc: i32,
}

/// Optional debug information attached to each prototype.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugInfo {
    pub line_defined: i32,
    pub last_line_defined: i32,
    pub source: Option<Vec<u8>>,
    pub name: Option<Vec<u8>>,
    pub line_info: Vec<i32>,
    pub locals: Vec<LocalVar>,
    pub upvalue_names: Vec<Option<Vec<u8>>>,
}

/// A compiled function.
#[derive(Debug, Clone, PartialEq)]
pub struct Proto {
    pub num_upvalues: u32,
    pub num_params: u32,
    pub vararg: u8,
    pub max_stack: u32,
    pub unknown: u32,
    pub code: Vec<Instruction>,
    pub constants: Vec<Constant>,
    pub debug: Option<DebugInfo>,
    pub protos: Vec<Proto>,
}

impl Proto {
    /// The debug name, or `?` when stripped.
    pub fn name(&self) -> String {
        self.debug
            .as_ref()
            .and_then(|d| d.name.as_deref())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_else(|| "?".to_owned())
    }

    /// Source line of instruction `pc`, when debug info is present.
    pub fn line(&self, pc: usize) -> Option<i32> {
        self.debug.as_ref()?.line_info.get(pc).copied()
    }

    /// Visits this prototype and all nested children depth first.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Proto)) {
        f(self);
        for child in &self.protos {
            child.walk(f);
        }
    }
}

/// A parsed `.hks` file.
#[derive(Debug, Clone, PartialEq)]
pub struct HksFile {
    pub header: Header,
    pub types: Vec<TypeEntry>,
    pub main: Proto,
}

pub fn is_hks(data: &[u8]) -> bool {
    data.starts_with(b"\x1bLuaQ\x0e")
}

/// Big-endian helpers over the shared little-endian reader.
struct Be<'a>(Reader<'a>);

impl<'a> Be<'a> {
    fn u8(&mut self) -> Result<u8> {
        self.0.u8()
    }

    fn u32(&mut self) -> Result<u32> {
        self.0.u32_be()
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.0.bytes(8)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// Count field, sanity-limited against the bytes that remain.
    fn count(&mut self, min_item_size: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        let remaining = self.0.data().len().saturating_sub(self.0.pos());
        if n.saturating_mul(min_item_size) > remaining {
            return bail(format!(
                "count {n} at {:#x} exceeds remaining {remaining} bytes",
                self.0.pos() - 4
            ));
        }
        Ok(n)
    }

    /// A `size_t`-prefixed string whose length includes the terminator; zero means absent.
    fn string(&mut self) -> Result<Option<Vec<u8>>> {
        let len = self.u64()?;
        if len == 0 {
            return Ok(None);
        }
        let len = usize::try_from(len).map_err(|_| Error::Format("string too long".into()))?;
        let bytes = self.0.bytes(len)?;
        if bytes[len - 1] != 0 {
            return bail(format!("unterminated string at {:#x}", self.0.pos() - len));
        }
        Ok(Some(bytes[..len - 1].to_vec()))
    }
}

pub fn parse(data: &[u8]) -> Result<HksFile> {
    let mut r = Be(Reader::new(data));
    r.0.magic(b"\x1bLua")?;
    let header = Header {
        version: r.u8()?,
        format: r.u8()?,
        little_endian: r.u8()? != 0,
        int_size: r.u8()?,
        size_t_size: r.u8()?,
        instruction_size: r.u8()?,
        number_size: r.u8()?,
        number_integral: r.u8()? != 0,
        build_flags: r.u8()?,
        reserved: r.u8()?,
    };
    if header.version != 0x51 || header.format != 0x0e {
        return bail(format!(
            "not HavokScript: version {:#x} format {:#x}",
            header.version, header.format
        ));
    }
    if header.little_endian
        || header.int_size != 4
        || header.size_t_size != 8
        || header.instruction_size != 4
        || header.number_size != 4
        || header.number_integral
    {
        return bail(format!("unsupported HKS build: {header:?}"));
    }
    let type_count = r.count(8)?;
    let mut types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let id = r.u32()?;
        let len = r.count(1)?;
        let bytes = r.0.bytes(len)?;
        let name = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        types.push(TypeEntry {
            id,
            name: String::from_utf8_lossy(name).into_owned(),
        });
    }
    let main = parse_proto(&mut r, 0)?;
    // Some files carry a short trailer after the main prototype; it is not interpreted.
    Ok(HksFile {
        header,
        types,
        main,
    })
}

fn parse_proto(r: &mut Be<'_>, depth: usize) -> Result<Proto> {
    if depth > 200 {
        return bail("prototype nesting too deep");
    }
    let num_upvalues = r.u32()?;
    let num_params = r.u32()?;
    let vararg = r.u8()?;
    let max_stack = r.u32()?;
    let unknown = r.u32()?;
    let code_len = r.count(4)?;
    let pad = (4 - r.0.pos() % 4) % 4;
    r.0.skip(pad);
    let mut code = Vec::with_capacity(code_len);
    for _ in 0..code_len {
        code.push(Instruction(r.u32()?));
    }
    let const_count = r.count(1)?;
    let mut constants = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        let tag = r.u8()?;
        constants.push(match tag {
            0 => Constant::Nil,
            1 => Constant::Bool(r.u8()? != 0),
            3 => Constant::Number(r.f32()?),
            4 => Constant::String(r.string()?.unwrap_or_default()),
            other => {
                return bail(format!(
                    "unsupported constant type {other} at {:#x}",
                    r.0.pos() - 1
                ));
            }
        });
    }
    let has_debug = r.u32()?;
    let debug = match has_debug {
        0 => None,
        1 => {
            let line_count = r.count(4)?;
            let local_count = r.count(16)?;
            let upvalue_count = r.count(8)?;
            let line_defined = r.i32()?;
            let last_line_defined = r.i32()?;
            let source = r.string()?;
            let name = r.string()?;
            let mut line_info = Vec::with_capacity(line_count);
            for _ in 0..line_count {
                line_info.push(r.i32()?);
            }
            let mut locals = Vec::with_capacity(local_count);
            for _ in 0..local_count {
                locals.push(LocalVar {
                    name: r.string()?,
                    start_pc: r.i32()?,
                    end_pc: r.i32()?,
                });
            }
            let mut upvalue_names = Vec::with_capacity(upvalue_count);
            for _ in 0..upvalue_count {
                upvalue_names.push(r.string()?);
            }
            Some(DebugInfo {
                line_defined,
                last_line_defined,
                source,
                name,
                line_info,
                locals,
                upvalue_names,
            })
        }
        other => return bail(format!("unexpected debug flag {other}")),
    };
    let child_count = r.count(25)?;
    let mut protos = Vec::with_capacity(child_count);
    for _ in 0..child_count {
        protos.push(parse_proto(r, depth + 1)?);
    }
    Ok(Proto {
        num_upvalues,
        num_params,
        vararg,
        max_stack,
        unknown,
        code,
        constants,
        debug,
        protos,
    })
}

/// HKS opcodes, numbered as the Sekiro build encodes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Op {
    GetField = 0,
    Test,
    CallI,
    CallC,
    Eq,
    EqBk,
    GetGlobal,
    Move,
    SelfOp,
    Return,
    GetTableS,
    GetTableN,
    GetTable,
    LoadBool,
    TForLoop,
    SetField,
    SetTableS,
    SetTableSBk,
    SetTableN,
    SetTableNBk,
    SetTable,
    SetTableBk,
    TailCallI,
    TailCallC,
    TailCallM,
    LoadK,
    LoadNil,
    SetGlobal,
    Jmp,
    CallM,
    Call,
    IntrinsicIndex,
    IntrinsicNewIndex,
    IntrinsicSelf,
    IntrinsicIndexLiteral,
    IntrinsicNewIndexLiteral,
    IntrinsicSelfLiteral,
    TailCall,
    GetUpval,
    SetUpval,
    Add,
    AddBk,
    Sub,
    SubBk,
    Mul,
    MulBk,
    Div,
    DivBk,
    Mod,
    ModBk,
    Pow,
    PowBk,
    NewTable,
    Unm,
    Not,
    Len,
    Lt,
    LtBk,
    Le,
    LeBk,
    Concat,
    TestSet,
    ForPrep,
    ForLoop,
    SetList,
    Close,
    Closure,
    VarArg,
    TailCallIR1,
    CallIR1,
    SetUpvalR1,
    TestR1,
    NotR1,
    GetFieldR1,
    SetFieldR1,
    NewStruct,
    Data,
    SetSlotN,
    SetSlotI,
    SetSlot,
    SetSlotS,
    SetSlotMt,
    CheckType,
    CheckTypeS,
    GetSlot,
    GetSlotMt,
    SelfSlot,
    SelfSlotMt,
    GetFieldMm,
    CheckTypeD,
    GetSlotD,
    GetGlobalMem,
}

/// Operand layout of an opcode, used for disassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpMode {
    Abc,
    ABx,
    AsBx,
}

impl Op {
    pub const COUNT: u8 = Op::GetGlobalMem as u8 + 1;

    pub fn from_u8(v: u8) -> Option<Op> {
        if v < Self::COUNT {
            // SAFETY: Op is repr(u8) with contiguous discriminants 0..COUNT.
            Some(unsafe { std::mem::transmute::<u8, Op>(v) })
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        OP_NAMES[self as usize]
    }

    pub fn mode(self) -> OpMode {
        match self {
            Op::LoadK
            | Op::SetGlobal
            | Op::GetGlobal
            | Op::GetGlobalMem
            | Op::Closure
            | Op::Data => OpMode::ABx,
            Op::Jmp | Op::ForPrep | Op::ForLoop => OpMode::AsBx,
            _ => OpMode::Abc,
        }
    }
}

const OP_NAMES: [&str; Op::COUNT as usize] = [
    "GETFIELD",
    "TEST",
    "CALL_I",
    "CALL_C",
    "EQ",
    "EQ_BK",
    "GETGLOBAL",
    "MOVE",
    "SELF",
    "RETURN",
    "GETTABLE_S",
    "GETTABLE_N",
    "GETTABLE",
    "LOADBOOL",
    "TFORLOOP",
    "SETFIELD",
    "SETTABLE_S",
    "SETTABLE_S_BK",
    "SETTABLE_N",
    "SETTABLE_N_BK",
    "SETTABLE",
    "SETTABLE_BK",
    "TAILCALL_I",
    "TAILCALL_C",
    "TAILCALL_M",
    "LOADK",
    "LOADNIL",
    "SETGLOBAL",
    "JMP",
    "CALL_M",
    "CALL",
    "INTRINSIC_INDEX",
    "INTRINSIC_NEWINDEX",
    "INTRINSIC_SELF",
    "INTRINSIC_INDEX_LITERAL",
    "INTRINSIC_NEWINDEX_LITERAL",
    "INTRINSIC_SELF_LITERAL",
    "TAILCALL",
    "GETUPVAL",
    "SETUPVAL",
    "ADD",
    "ADD_BK",
    "SUB",
    "SUB_BK",
    "MUL",
    "MUL_BK",
    "DIV",
    "DIV_BK",
    "MOD",
    "MOD_BK",
    "POW",
    "POW_BK",
    "NEWTABLE",
    "UNM",
    "NOT",
    "LEN",
    "LT",
    "LT_BK",
    "LE",
    "LE_BK",
    "CONCAT",
    "TESTSET",
    "FORPREP",
    "FORLOOP",
    "SETLIST",
    "CLOSE",
    "CLOSURE",
    "VARARG",
    "TAILCALL_I_R1",
    "CALL_I_R1",
    "SETUPVAL_R1",
    "TEST_R1",
    "NOT_R1",
    "GETFIELD_R1",
    "SETFIELD_R1",
    "NEWSTRUCT",
    "DATA",
    "SETSLOTN",
    "SETSLOTI",
    "SETSLOT",
    "SETSLOTS",
    "SETSLOTMT",
    "CHECKTYPE",
    "CHECKTYPES",
    "GETSLOT",
    "GETSLOTMT",
    "SELFSLOT",
    "SELFSLOTMT",
    "GETFIELD_MM",
    "CHECKTYPE_D",
    "GETSLOT_D",
    "GETGLOBAL_MEM",
];

/// Number of inline-cache `DATA` words the compiler emits after an instruction.
///
/// Cached global loads carry one slot and table field reads two; `CLOSURE` is instead followed
/// by one `DATA` per upvalue of the child prototype. The interpreter treats `DATA` as a no-op.
pub fn trailing_data_words(op: Op) -> usize {
    match op {
        Op::GetGlobalMem => 1,
        Op::GetTableS | Op::GetFieldR1 => 2,
        _ => 0,
    }
}

/// Bit 8 of a 9-bit C operand marks a constant index (Lua's RK encoding).
pub const RK_CONSTANT: u32 = 0x100;

/// Bias of the signed jump operand, as in Lua 5.1 with a 17-bit Bx.
pub const SBX_BIAS: i32 = 0xFFFF;

/// One 32-bit HKS instruction.
///
/// Bit layout: `op:7 | B:8 | C:9 | A:8` from high to low; `Bx` is the 17 bits of B and C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instruction(pub u32);

impl Instruction {
    pub fn raw_op(self) -> u8 {
        (self.0 >> 25) as u8
    }

    pub fn op(self) -> Option<Op> {
        Op::from_u8(self.raw_op())
    }

    pub fn a(self) -> u32 {
        self.0 & 0xFF
    }

    pub fn b(self) -> u32 {
        (self.0 >> 17) & 0xFF
    }

    /// The raw 9-bit C operand, including the constant flag.
    pub fn c(self) -> u32 {
        (self.0 >> 8) & 0x1FF
    }

    pub fn bx(self) -> u32 {
        (self.0 >> 8) & 0x1FFFF
    }

    pub fn sbx(self) -> i32 {
        self.bx() as i32 - SBX_BIAS
    }
}

/// Renders a register-or-constant operand.
fn rk(proto: &Proto, v: u32) -> String {
    if v & RK_CONSTANT != 0 {
        konst(proto, v & 0xFF)
    } else {
        format!("R{v}")
    }
}

fn konst(proto: &Proto, idx: u32) -> String {
    match proto.constants.get(idx as usize) {
        Some(c) => c.to_string(),
        None => format!("K{idx}?"),
    }
}

/// Human-readable one-line rendering of instruction `pc` of `proto`.
pub fn disassemble_instruction(proto: &Proto, pc: usize) -> String {
    let ins = proto.code[pc];
    let Some(op) = ins.op() else {
        return format!("<bad opcode {}> {:#010x}", ins.raw_op(), ins.0);
    };
    let (a, b, c, bx) = (ins.a(), ins.b(), ins.c(), ins.bx());
    let operands = match op.mode() {
        OpMode::Abc => format!("{a} {b} {c}"),
        OpMode::ABx => format!("{a} {bx}"),
        OpMode::AsBx => format!("{a} {}", ins.sbx()),
    };
    let target = |pc: usize| pc as i64 + 1 + ins.sbx() as i64;
    let note = match op {
        Op::Move => format!("R{a} = R{b}"),
        Op::LoadK => format!("R{a} = {}", konst(proto, bx)),
        Op::LoadBool => format!("R{a} = {}{}", b != 0, if c != 0 { "; skip" } else { "" }),
        Op::LoadNil => format!("R{a}..R{b} = nil"),
        Op::GetGlobal | Op::GetGlobalMem => format!("R{a} = _G[{}]", konst(proto, bx)),
        Op::SetGlobal => format!("_G[{}] = R{a}", konst(proto, bx)),
        Op::GetField | Op::GetFieldR1 => format!("R{a} = R{b}[{}]", konst(proto, c)),
        Op::SetField | Op::SetFieldR1 => format!("R{a}[{}] = {}", konst(proto, b), rk(proto, c)),
        Op::GetTable | Op::GetTableS => format!("R{a} = R{b}[{}]", rk(proto, c)),
        Op::SetTable | Op::SetTableS => format!("R{a}[R{b}] = {}", rk(proto, c)),
        Op::SetTableSBk => format!("R{a}[{}] = {}", konst(proto, b), rk(proto, c)),
        Op::GetUpval => format!("R{a} = U{b}"),
        Op::SetUpval | Op::SetUpvalR1 => format!("U{b} = R{a}"),
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
            format!("R{a} = R{b} {} {}", arith_symbol(op), rk(proto, c))
        }
        Op::AddBk | Op::SubBk | Op::MulBk | Op::DivBk | Op::ModBk | Op::PowBk => {
            format!("R{a} = {} {} R{c}", konst(proto, b), arith_symbol(op))
        }
        Op::Unm => format!("R{a} = -R{b}"),
        Op::Not | Op::NotR1 => format!("R{a} = not R{b}"),
        Op::Len => format!("R{a} = #R{b}"),
        Op::Concat => format!("R{a} = R{b} .. ... .. R{c}"),
        Op::Jmp => format!("goto {}", target(pc)),
        Op::Eq => format!("if (R{b} == {}) ~= {a} then skip", rk(proto, c)),
        Op::Lt => format!("if (R{b} < {}) ~= {a} then skip", rk(proto, c)),
        Op::Le => format!("if (R{b} <= {}) ~= {a} then skip", rk(proto, c)),
        Op::EqBk => format!("if ({} == R{c}) ~= {a} then skip", konst(proto, b)),
        Op::LtBk => format!("if ({} < R{c}) ~= {a} then skip", konst(proto, b)),
        Op::LeBk => format!("if ({} <= R{c}) ~= {a} then skip", konst(proto, b)),
        Op::Test | Op::TestR1 => format!("if truthy(R{a}) ~= {c} then skip"),
        Op::Call | Op::CallI | Op::CallIR1 | Op::CallC | Op::CallM => {
            format!("R{a}.. = R{a}(args {b}) results {c}")
        }
        Op::TailCall | Op::TailCallI | Op::TailCallIR1 | Op::TailCallC | Op::TailCallM => {
            format!("return R{a}(args {b})")
        }
        Op::Return => format!("return R{a} count {b}"),
        Op::ForPrep => format!("for prep, goto {}", target(pc)),
        Op::ForLoop => format!("for loop, back to {}", target(pc)),
        Op::Closure => {
            let name = proto
                .protos
                .get(bx as usize)
                .map(|p| p.name())
                .unwrap_or_else(|| "?".into());
            format!("R{a} = closure #{bx} ({name})")
        }
        Op::Data => format!("data {a} {bx}"),
        _ => String::new(),
    };
    format!("{:<14} {:<12} ; {note}", op.name(), operands)
}

fn arith_symbol(op: Op) -> &'static str {
    match op {
        Op::Add | Op::AddBk => "+",
        Op::Sub | Op::SubBk => "-",
        Op::Mul | Op::MulBk => "*",
        Op::Div | Op::DivBk => "/",
        Op::Mod | Op::ModBk => "%",
        _ => "^",
    }
}

/// Full listing of a prototype and its children.
pub fn disassemble(proto: &Proto) -> String {
    let mut out = String::new();
    disassemble_into(proto, "main", &mut out);
    out
}

fn disassemble_into(proto: &Proto, path: &str, out: &mut String) {
    let _ = writeln!(
        out,
        "function {path} {} (params {}, upvalues {}, stack {}, {} instructions, {} constants)",
        proto.name(),
        proto.num_params,
        proto.num_upvalues,
        proto.max_stack,
        proto.code.len(),
        proto.constants.len()
    );
    for pc in 0..proto.code.len() {
        let line = proto.line(pc).unwrap_or(0);
        let _ = writeln!(
            out,
            "  {pc:5} [{line:5}] {}",
            disassemble_instruction(proto, pc)
        );
    }
    for (i, child) in proto.protos.iter().enumerate() {
        disassemble_into(child, &format!("{path}.{i}"), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_fields() {
        // CLOSURE A=0 Bx=0, then SETGLOBAL A=0 Bx=0: the first two words of the player script.
        let closure = Instruction(0x8400_0000);
        assert_eq!(closure.op(), Some(Op::Closure));
        assert_eq!((closure.a(), closure.bx()), (0, 0));
        let setglobal = Instruction(0x3600_0100);
        assert_eq!(setglobal.op(), Some(Op::SetGlobal));
        assert_eq!(setglobal.bx(), 1);
        // A forward jump of 3 encodes Bx = 0xFFFF + 3.
        let jmp = Instruction(((Op::Jmp as u32) << 25) | ((0xFFFF + 3) << 8));
        assert_eq!(jmp.sbx(), 3);
    }

    #[test]
    fn op_table_is_complete() {
        assert_eq!(Op::from_u8(91), Some(Op::GetGlobalMem));
        assert_eq!(Op::from_u8(92), None);
        assert_eq!(Op::Closure as u8, 66);
        assert_eq!(Op::Data as u8, 76);
        assert_eq!(Op::GetGlobalMem.name(), "GETGLOBAL_MEM");
    }
}
