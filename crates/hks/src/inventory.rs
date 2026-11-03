//! Static inventory of the calls a script makes, read straight from the bytecode.
//!
//! For each call instruction it recovers the callee (when it was loaded from a global) and the
//! shape of each argument (literal number or string, a named global, or a computed value).
//! Tracking is per basic block, so values are never carried across a branch.

use sekiro_formats::hks::{Constant, Op, Proto, RK_CONSTANT, trailing_data_words};
use std::collections::BTreeSet;

/// What is known about a register when a call is made.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Number(f32),
    Str(String),
    Bool(bool),
    Nil,
    /// The value of a named global, such as a constant from the define script.
    Global(String),
    /// A value computed at run time.
    Computed,
}

#[derive(Debug, Clone)]
pub struct CallSite {
    pub chunk: String,
    pub function: String,
    pub pc: usize,
    pub line: i32,
    /// Global name the callee was loaded from, if any.
    pub callee: Option<String>,
    /// Arguments; `None` when the argument count is only known at run time.
    pub args: Option<Vec<Arg>>,
}

/// Global names read and written by a chunk.
#[derive(Debug, Default)]
pub struct GlobalUse {
    pub read: BTreeSet<String>,
    pub written: BTreeSet<String>,
}

fn constant_arg(c: &Constant) -> Arg {
    match c {
        Constant::Nil => Arg::Nil,
        Constant::Bool(b) => Arg::Bool(*b),
        Constant::Number(n) => Arg::Number(*n),
        Constant::String(s) => Arg::Str(String::from_utf8_lossy(s).into_owned()),
    }
}

fn constant_name(proto: &Proto, idx: u32) -> Option<String> {
    match proto.constants.get(idx as usize)? {
        Constant::String(s) => Some(String::from_utf8_lossy(s).into_owned()),
        _ => None,
    }
}

/// Instruction indices that start a basic block (jump targets and fall-throughs after tests).
fn leaders(proto: &Proto) -> Vec<bool> {
    let n = proto.code.len();
    let mut lead = vec![false; n + 1];
    lead[0] = true;
    for (pc, ins) in proto.code.iter().enumerate() {
        match ins.op() {
            Some(Op::Jmp | Op::ForPrep | Op::ForLoop) => {
                let t = pc as i64 + 1 + ins.sbx() as i64;
                if (0..=n as i64).contains(&t) {
                    lead[t as usize] = true;
                }
                lead[pc + 1] = true;
            }
            Some(
                Op::Eq
                | Op::EqBk
                | Op::Lt
                | Op::LtBk
                | Op::Le
                | Op::LeBk
                | Op::Test
                | Op::TestR1
                | Op::TestSet
                | Op::LoadBool
                | Op::TForLoop,
            ) => {
                lead[(pc + 2).min(n)] = true;
                lead[pc + 1] = true;
            }
            _ => {}
        }
    }
    lead
}

/// Scans `proto` and its children, appending call sites and global use.
pub fn scan(proto: &Proto, chunk: &str, calls: &mut Vec<CallSite>, globals: &mut GlobalUse) {
    let lead = leaders(proto);
    let mut regs: Vec<Arg> = vec![Arg::Computed; proto.max_stack as usize + 256];
    let mut pc = 0;
    while pc < proto.code.len() {
        let ins = proto.code[pc];
        if lead[pc] {
            regs.iter_mut().for_each(|r| *r = Arg::Computed);
        }
        let a = ins.a() as usize;
        let Some(op) = ins.op() else {
            pc += 1;
            continue;
        };
        match op {
            Op::LoadK => regs[a] = constant_arg(&proto.constants[ins.bx() as usize]),
            Op::LoadBool => regs[a] = Arg::Bool(ins.b() != 0),
            Op::LoadNil => {
                regs[a..=ins.b() as usize].fill(Arg::Nil);
            }
            Op::Move => regs[a] = regs[ins.b() as usize].clone(),
            Op::GetGlobal | Op::GetGlobalMem => {
                let name = constant_name(proto, ins.bx()).unwrap_or_default();
                globals.read.insert(name.clone());
                regs[a] = Arg::Global(name);
            }
            Op::SetGlobal => {
                globals
                    .written
                    .insert(constant_name(proto, ins.bx()).unwrap_or_default());
            }
            Op::GetField | Op::GetFieldR1 => {
                let field = constant_name(proto, ins.c()).unwrap_or_default();
                regs[a] = match &regs[ins.b() as usize] {
                    Arg::Global(t) => Arg::Global(format!("{t}.{field}")),
                    _ => Arg::Computed,
                };
            }
            Op::GetTable | Op::GetTableS => {
                let c = ins.c();
                let key = if c & RK_CONSTANT != 0 {
                    proto.constants.get((c & 0xFF) as usize).map(constant_arg)
                } else {
                    Some(regs[c as usize].clone())
                };
                regs[a] = match (&regs[ins.b() as usize], key) {
                    (Arg::Global(t), Some(Arg::Str(field))) => Arg::Global(format!("{t}.{field}")),
                    _ => Arg::Computed,
                };
            }
            Op::Call
            | Op::CallI
            | Op::CallIR1
            | Op::CallC
            | Op::CallM
            | Op::TailCall
            | Op::TailCallI
            | Op::TailCallIR1
            | Op::TailCallC
            | Op::TailCallM => {
                let callee = match &regs[a] {
                    Arg::Global(name) => Some(name.clone()),
                    _ => None,
                };
                let b = ins.b() as usize;
                let args = (b != 0).then(|| regs[a + 1..a + b].to_vec());
                calls.push(CallSite {
                    chunk: chunk.to_owned(),
                    function: proto.name(),
                    pc,
                    line: proto.line(pc).unwrap_or(0),
                    callee,
                    args,
                });
                regs[a..].iter_mut().for_each(|r| *r = Arg::Computed);
            }
            Op::Closure => {
                regs[a] = Arg::Computed;
                pc += proto.protos[ins.bx() as usize].num_upvalues as usize;
            }
            Op::SelfOp => {
                regs[a] = Arg::Computed;
                regs[a + 1] = Arg::Computed;
            }
            Op::TForLoop => regs[a + 2..].iter_mut().for_each(|r| *r = Arg::Computed),
            Op::VarArg => regs[a..].iter_mut().for_each(|r| *r = Arg::Computed),
            Op::Jmp
            | Op::Eq
            | Op::EqBk
            | Op::Lt
            | Op::LtBk
            | Op::Le
            | Op::LeBk
            | Op::Test
            | Op::TestR1
            | Op::Return
            | Op::SetField
            | Op::SetFieldR1
            | Op::SetTable
            | Op::SetTableS
            | Op::SetTableSBk
            | Op::SetList
            | Op::SetUpval
            | Op::SetUpvalR1
            | Op::Close
            | Op::Data => {}
            Op::ForPrep | Op::ForLoop => {
                regs[a..a + 4].fill(Arg::Computed);
            }
            _ => regs[a] = Arg::Computed,
        }
        pc += 1 + trailing_data_words(op);
    }
    for child in &proto.protos {
        scan(child, chunk, calls, globals);
    }
}
