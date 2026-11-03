//! Event scripts (`event/*.emevd`, magic `EVD\0`, version 0xCD, 64-bit): the per-map and common
//! scripts that drive boss fights, flags, cutscenes and map state.
//!
//! Layout from SoulsFormats `EMEVD` (the DS3 / Sekiro form), reimplemented:
//!
//! - header: `EVD\0`, byte 0 (little endian), 0xFF (64-bit), 1, 0xFF, `u32` version 0xCD,
//!   `u32` file size, then pairs of 64-bit count and offset for events, instructions, an unused
//!   table, layers, parameters, linked files, the argument blob and the string blob;
//! - event (0x30 bytes): `i64` id, instruction count and offset (into the instruction table),
//!   parameter count and offset (into the parameter table, -1 for none), `u32` rest behaviour,
//!   `u32` 0;
//! - instruction (0x20 bytes): `i32` bank, `i32` id, `i64` argument length, `i64` argument offset
//!   (into the argument blob), `i64` layer offset (-1 for none);
//! - parameter (0x20 bytes): `i64` instruction index within its event, `i64` target byte in that
//!   instruction's arguments, `i64` source byte in the arguments the event was started with,
//!   `i32` byte count, `i32` 0;
//! - layer (0x20 bytes): `i32` 2, `u32` layer mask, then padding.
//!
//! An event is started by another event's `InitializeEvent` (bank 2000, id 0: slot, event id,
//! arguments) or `InitializeCommonEvent` (2000[6]: event id, arguments; for events in
//! `common_func.emevd`). A parameter copies `byte_count` bytes from those arguments (after the
//! slot and event id) into the instruction's
//! own arguments; [`Event::resolve`] applies them.
//!
//! The argument types and names of each instruction are not in the file; they come from the
//! community instruction tables (DarkScript3's EMEDF for Sekiro). [`crate::emedf`] holds the
//! subset we use.

use crate::reader::{Reader, Result, bail, slice};

/// What happens when an event is reloaded (map reload, death).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestBehavior {
    Default,
    Restart,
    End,
    Other(u32),
}

impl RestBehavior {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Default,
            1 => Self::Restart,
            2 => Self::End,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub bank: i32,
    pub id: i32,
    /// The raw argument bytes, packed with natural alignment and padded to 4 bytes.
    pub args: Vec<u8>,
    /// Layer mask, when the instruction only runs in some map layers.
    pub layer: Option<u32>,
}

impl Instruction {
    pub fn i32_at(&self, offset: usize) -> Option<i32> {
        self.args
            .get(offset..offset + 4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u32_at(&self, offset: usize) -> Option<u32> {
        self.i32_at(offset).map(|v| v as u32)
    }

    pub fn f32_at(&self, offset: usize) -> Option<f32> {
        self.i32_at(offset).map(|v| f32::from_bits(v as u32))
    }

    pub fn i16_at(&self, offset: usize) -> Option<i16> {
        self.args
            .get(offset..offset + 2)
            .map(|b| i16::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u8_at(&self, offset: usize) -> Option<u8> {
        self.args.get(offset).copied()
    }

    pub fn i8_at(&self, offset: usize) -> Option<i8> {
        self.u8_at(offset).map(|v| v as i8)
    }

    /// The arguments as little-endian 32-bit words (a trailing partial word is dropped).
    pub fn words(&self) -> impl Iterator<Item = i32> + '_ {
        self.args
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| i32::from_le_bytes(*b))
    }

    /// `bank[id]`, the usual way the instruction tables name an instruction.
    pub fn code(&self) -> (i32, i32) {
        (self.bank, self.id)
    }
}

/// A substitution of the event's start arguments into one of its instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    pub instruction: usize,
    pub target_start: usize,
    pub source_start: usize,
    pub byte_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub rest: RestBehavior,
    pub instructions: Vec<Instruction>,
    pub parameters: Vec<Parameter>,
}

impl Event {
    /// The instructions with the parameters filled in from `start_args` (the starting
    /// instruction's arguments after its slot and event id words). Bytes the start arguments do
    /// not cover are left as stored.
    pub fn resolve(&self, start_args: &[u8]) -> Vec<Instruction> {
        let mut out = self.instructions.clone();
        for p in &self.parameters {
            let Some(ins) = out.get_mut(p.instruction) else {
                continue;
            };
            let Some(src) = start_args.get(p.source_start..p.source_start + p.byte_count) else {
                continue;
            };
            if let Some(dst) = ins
                .args
                .get_mut(p.target_start..p.target_start + p.byte_count)
            {
                dst.copy_from_slice(src);
            }
        }
        out
    }

    /// Whether the instruction at `index` has any of its bytes replaced by a parameter.
    pub fn parameters_of(&self, index: usize) -> impl Iterator<Item = &Parameter> {
        self.parameters
            .iter()
            .filter(move |p| p.instruction == index)
    }
}

/// One start of an event: which event, which slot, and the arguments passed.
#[derive(Debug, Clone, PartialEq)]
pub struct EventStart {
    /// The event holding the starting instruction.
    pub from_event: i64,
    /// The slot (0 for a common event start, which has none).
    pub slot: i32,
    pub event: i64,
    /// The arguments after the slot and event id words.
    pub args: Vec<u8>,
    /// `false` for `InitializeEvent` (2000[0]), `true` for `InitializeCommonEvent` (2000[6]).
    pub common: bool,
}

impl EventStart {
    pub fn arg_i32(&self, index: usize) -> Option<i32> {
        self.args
            .get(index * 4..index * 4 + 4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Emevd {
    pub events: Vec<Event>,
    /// Offsets of linked files' names in the string blob.
    pub linked_files: Vec<u64>,
    /// The string blob (UTF-16 file names of linked scripts).
    pub strings: Vec<u8>,
}

const EVENT_SIZE: usize = 0x30;
const INSTRUCTION_SIZE: usize = 0x20;
const PARAMETER_SIZE: usize = 0x20;

impl Emevd {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        r.magic(b"EVD\0")?;
        let flags = r.bytes(4)?;
        if flags[0] != 0 || flags[1] != 0xFF {
            return bail(format!(
                "unsupported EMEVD flags {flags:02x?} (only little-endian 64-bit)"
            ));
        }
        let version = r.u32()?;
        if version != 0xCD && version != 0xCC {
            return bail(format!("unsupported EMEVD version {version:#x}"));
        }
        let _file_size = r.u32()?;
        let pair = |r: &mut Reader| -> Result<(usize, usize)> {
            let count = r.offset64()?;
            let offset = r.offset64()?;
            Ok((count, offset))
        };
        let (event_count, events_at) = pair(&mut r)?;
        let (instruction_count, instructions_at) = pair(&mut r)?;
        let _unused = pair(&mut r)?;
        let (_layer_count, _layers_at) = pair(&mut r)?;
        let (parameter_count, parameters_at) = pair(&mut r)?;
        let (linked_count, linked_at) = pair(&mut r)?;
        let (args_len, args_at) = pair(&mut r)?;
        let (strings_len, strings_at) = pair(&mut r)?;

        let args_blob = slice(data, args_at, args_len)?;
        let strings = slice(data, strings_at, strings_len)?.to_vec();

        let mut linked_files = Vec::with_capacity(linked_count);
        let mut lr = Reader::at(data, linked_at);
        for _ in 0..linked_count {
            linked_files.push(lr.u64()?);
        }

        let mut events = Vec::with_capacity(event_count);
        for e in 0..event_count {
            let mut er = Reader::at(data, events_at + e * EVENT_SIZE);
            let id = er.i64()?;
            let ins_count = er.offset64()?;
            let ins_offset = er.offset64()?;
            let par_count = er.offset64()?;
            let par_offset = er.i64()?;
            let rest = RestBehavior::from_u32(er.u32()?);

            if ins_offset % INSTRUCTION_SIZE != 0
                || ins_offset / INSTRUCTION_SIZE + ins_count > instruction_count
            {
                return bail(format!("event {id}: instructions out of range"));
            }
            let mut instructions = Vec::with_capacity(ins_count);
            for i in 0..ins_count {
                let mut ir = Reader::at(data, instructions_at + ins_offset + i * INSTRUCTION_SIZE);
                let bank = ir.i32()?;
                let code = ir.i32()?;
                let len = ir.offset64()?;
                let off = ir.i64()?;
                let layer_off = ir.i64()?;
                let args = match usize::try_from(off) {
                    Ok(off) if len > 0 => slice(args_blob, off, len)?.to_vec(),
                    _ if len == 0 => Vec::new(),
                    _ => return bail(format!("event {id}: arguments without an offset")),
                };
                let layer = if layer_off < 0 {
                    None
                } else {
                    // Layer offsets point into the layer table, relative to its start.
                    let mut lr = Reader::at(data, _layers_at + layer_off as usize);
                    let _two = lr.i32()?;
                    Some(lr.u32()?)
                };
                instructions.push(Instruction {
                    bank,
                    id: code,
                    args,
                    layer,
                });
            }

            let mut parameters = Vec::with_capacity(par_count);
            if par_count > 0 {
                let Ok(par_offset) = usize::try_from(par_offset) else {
                    return bail(format!("event {id}: parameters without an offset"));
                };
                if par_offset / PARAMETER_SIZE + par_count > parameter_count {
                    return bail(format!("event {id}: parameters out of range"));
                }
                for p in 0..par_count {
                    let mut pr = Reader::at(data, parameters_at + par_offset + p * PARAMETER_SIZE);
                    parameters.push(Parameter {
                        instruction: pr.offset64()?,
                        target_start: pr.offset64()?,
                        source_start: pr.offset64()?,
                        byte_count: pr.offset32()?,
                    });
                }
            }
            events.push(Event {
                id,
                rest,
                instructions,
                parameters,
            });
        }
        Ok(Self {
            events,
            linked_files,
            strings,
        })
    }

    pub fn event(&self, id: i64) -> Option<&Event> {
        self.events.iter().find(|e| e.id == id)
    }

    /// The names of linked scripts (such as `N:\...\common_func.emevd`), from the string blob.
    pub fn linked_file_names(&self) -> Vec<String> {
        self.linked_files
            .iter()
            .filter_map(|&o| crate::reader::utf16z(&self.strings, o as usize).ok())
            .collect()
    }

    /// Every `InitializeEvent` / `InitializeCommonEvent` in the file, in order.
    pub fn starts(&self) -> Vec<EventStart> {
        let mut out = Vec::new();
        for e in &self.events {
            for ins in &e.instructions {
                // InitializeEvent: slot, event id, arguments. InitializeCommonEvent: event id,
                // arguments (the event lives in `common_func.emevd`).
                let (common, slot, event, args_at) = match ins.code() {
                    (2000, 0) => (false, ins.i32_at(0), ins.u32_at(4), 8),
                    (2000, 6) => (true, Some(0), ins.u32_at(0), 4),
                    _ => continue,
                };
                let (Some(slot), Some(event)) = (slot, event) else {
                    continue;
                };
                out.push(EventStart {
                    from_event: e.id,
                    slot,
                    event: event as i64,
                    args: ins.args.get(args_at..).unwrap_or_default().to_vec(),
                    common,
                });
            }
        }
        out
    }

    /// The starts of event `id`.
    pub fn starts_of(&self, id: i64) -> Vec<EventStart> {
        self.starts()
            .into_iter()
            .filter(|s| s.event == id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a tiny EMEVD: event 0 starts event 100 with argument 1110850; event 100 has one
    /// instruction whose first word is replaced by that argument.
    fn sample() -> Vec<u8> {
        let mut args = Vec::new();
        // Event 0, instruction 0: InitializeEvent(slot 0, event 100, 1110850).
        args.extend_from_slice(&0i32.to_le_bytes());
        args.extend_from_slice(&100u32.to_le_bytes());
        args.extend_from_slice(&1110850i32.to_le_bytes());
        args.extend_from_slice(&0i32.to_le_bytes()); // pad to 16
        // Event 100, instruction 0: SetSpEffect(0, 200051).
        let a1 = args.len();
        args.extend_from_slice(&0i32.to_le_bytes());
        args.extend_from_slice(&200051i32.to_le_bytes());

        let header = 0x90usize;
        let events_at = header;
        let instructions_at = events_at + 2 * EVENT_SIZE;
        let parameters_at = instructions_at + 2 * INSTRUCTION_SIZE;
        let args_at = parameters_at + PARAMETER_SIZE;
        let strings_at = args_at + args.len();
        let total = strings_at;

        let mut d = Vec::new();
        d.extend_from_slice(b"EVD\0");
        d.extend_from_slice(&[0, 0xFF, 1, 0xFF]);
        d.extend_from_slice(&0xCDu32.to_le_bytes());
        d.extend_from_slice(&(total as u32).to_le_bytes());
        let pairs: [(u64, u64); 8] = [
            (2, events_at as u64),
            (2, instructions_at as u64),
            (0, args_at as u64),
            (0, args_at as u64),
            (1, parameters_at as u64),
            (0, strings_at as u64),
            (args.len() as u64, args_at as u64),
            (0, strings_at as u64),
        ];
        for (c, o) in pairs {
            d.extend_from_slice(&c.to_le_bytes());
            d.extend_from_slice(&o.to_le_bytes());
        }
        assert_eq!(d.len(), header);
        // Events.
        for (id, ins_off, par_count, par_off) in [(0i64, 0u64, 0u64, -1i64), (100, 0x20, 1, 0)] {
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&1u64.to_le_bytes());
            d.extend_from_slice(&ins_off.to_le_bytes());
            d.extend_from_slice(&par_count.to_le_bytes());
            d.extend_from_slice(&par_off.to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
        }
        // Instructions.
        for (bank, id, len, off) in [(2000i32, 0i32, 12u64, 0u64), (2004, 8, 8, a1 as u64)] {
            d.extend_from_slice(&bank.to_le_bytes());
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&len.to_le_bytes());
            d.extend_from_slice(&off.to_le_bytes());
            d.extend_from_slice(&(-1i64).to_le_bytes());
        }
        // Parameter: instruction 0, target 0, source 0, 4 bytes.
        d.extend_from_slice(&0u64.to_le_bytes());
        d.extend_from_slice(&0u64.to_le_bytes());
        d.extend_from_slice(&0u64.to_le_bytes());
        d.extend_from_slice(&4i32.to_le_bytes());
        d.extend_from_slice(&0i32.to_le_bytes());
        d.extend_from_slice(&args);
        d
    }

    #[test]
    fn parses_events_instructions_and_parameters() {
        let e = Emevd::parse(&sample()).unwrap();
        assert_eq!(e.events.len(), 2);
        let ev = e.event(100).unwrap();
        assert_eq!(ev.rest, RestBehavior::Default);
        assert_eq!(ev.instructions[0].code(), (2004, 8));
        assert_eq!(ev.instructions[0].i32_at(4), Some(200051));
        assert_eq!(
            ev.parameters,
            vec![Parameter {
                instruction: 0,
                target_start: 0,
                source_start: 0,
                byte_count: 4
            }]
        );
    }

    #[test]
    fn finds_starts_and_resolves_parameters() {
        let e = Emevd::parse(&sample()).unwrap();
        let starts = e.starts_of(100);
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].arg_i32(0), Some(1110850));
        assert!(!starts[0].common);
        let resolved = e.event(100).unwrap().resolve(&starts[0].args);
        assert_eq!(resolved[0].i32_at(0), Some(1110850));
        assert_eq!(resolved[0].i32_at(4), Some(200051));
    }

    #[test]
    fn rejects_other_magic() {
        assert!(Emevd::parse(b"EVF\0\0\xff\x01\xff").is_err());
    }
}
