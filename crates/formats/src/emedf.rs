//! Names and argument layouts for the EMEVD instructions we read (a subset of the Sekiro
//! instruction table).
//!
//! The names and argument types are facts from the community instruction documentation
//! (DarkScript3's `sekiro-common.emedf.json`, after the soulsmods EMEDF tables); the table here
//! is written out by hand for the instructions the boss events use. Type codes and enums
//! (comparison 0 equal, 1 not equal, 2 greater, 3 less, 4 greater or equal, 5 less or equal;
//! condition groups -15..15) are the same tables'.
//!
//! Arguments are packed in order, each aligned to its own size, and the whole list is padded to
//! 4 bytes (SoulsFormats `EMEVD.Instruction` argument packing).

use crate::emevd::Instruction;

/// An argument type, as the instruction tables number them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    U8,
    U16,
    U32,
    I8,
    I16,
    I32,
    F32,
}

impl ArgType {
    fn size(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
        }
    }

    fn from_code(c: u8) -> Self {
        match c {
            b'0' => Self::U8,
            b'1' => Self::U16,
            b'2' => Self::U32,
            b'3' => Self::I8,
            b'4' => Self::I16,
            b'6' => Self::F32,
            _ => Self::I32,
        }
    }
}

/// A decoded argument value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arg {
    Int(i64),
    Float(f32),
}

impl Arg {
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Int(v) => v,
            Self::Float(v) => v as i64,
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            Self::Int(v) => v as f32,
            Self::Float(v) => v,
        }
    }
}

impl std::fmt::Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
        }
    }
}

/// One instruction's documentation: name and argument names with their type codes
/// (`0` u8, `1` u16, `2` u32, `3` i8, `4` i16, `5` i32, `6` f32).
pub struct Def {
    pub bank: i32,
    pub id: i32,
    pub name: &'static str,
    pub args: &'static [(&'static str, u8)],
}

macro_rules! defs {
    ($(($bank:expr, $id:expr, $name:expr, [$(($an:expr, $at:expr)),* $(,)?])),* $(,)?) => {
        &[$(Def { bank: $bank, id: $id, name: $name, args: &[$(($an, $at)),*] }),*]
    };
}

/// The instructions we name. `rg` is a condition group (0 main, positive AND, negative OR).
/// `IfCharacterDeadalive`'s `dead` is DarkScript3's death state (0 alive, 1 dead);
/// `IfCharacterHasEventMessage` waits for a TAE `SetEzStateRequestID` (231) message on the
/// character's animation. `IfInoutsideArea` (3[2]) is named after the DS3 table; its Sekiro
/// layout is assumed to match.
pub const DEFS: &[Def] = defs![
    (
        0,
        0,
        "IfConditionGroup",
        [("rg", b'3'), ("state", b'0'), ("group", b'3')]
    ),
    (
        3,
        0,
        "IfEventFlag",
        [
            ("rg", b'3'),
            ("state", b'0'),
            ("type", b'0'),
            ("flag", b'5')
        ]
    ),
    (
        3,
        1,
        "IfEventFlagRange",
        [
            ("rg", b'3'),
            ("state", b'0'),
            ("type", b'0'),
            ("first", b'5'),
            ("last", b'5')
        ]
    ),
    (
        3,
        8,
        "IfPlayerInMap",
        [
            ("rg", b'3'),
            ("inside", b'0'),
            ("area", b'0'),
            ("block", b'0')
        ]
    ),
    (
        3,
        2,
        "IfInoutsideArea",
        [
            ("rg", b'3'),
            ("inside", b'0'),
            ("entity", b'5'),
            ("area", b'5'),
            ("count", b'5')
        ]
    ),
    (
        3,
        3,
        "IfEntityInoutsideRadius",
        [
            ("rg", b'3'),
            ("inside", b'0'),
            ("entity", b'5'),
            ("target", b'5'),
            ("radius", b'6'),
            ("count", b'5')
        ]
    ),
    (
        3,
        31,
        "IfOngoingCutsceneFinished",
        [("rg", b'3'), ("cutscene", b'5')]
    ),
    (
        4,
        0,
        "IfCharacterDeadalive",
        [
            ("rg", b'3'),
            ("entity", b'5'),
            ("dead", b'0'),
            ("cmp", b'3'),
            ("count", b'6')
        ]
    ),
    (
        4,
        5,
        "IfCharacterHasSpeffect",
        [
            ("rg", b'3'),
            ("entity", b'5'),
            ("speffect", b'5'),
            ("has", b'0'),
            ("cmp", b'3'),
            ("count", b'6')
        ]
    ),
    (
        4,
        8,
        "IfCharacterHasEventMessage",
        [
            ("rg", b'3'),
            ("entity", b'5'),
            ("message", b'5'),
            ("has", b'0'),
            ("cmp", b'3'),
            ("count", b'6')
        ]
    ),
    (
        4,
        14,
        "IfCharacterHPValue",
        [
            ("rg", b'3'),
            ("entity", b'5'),
            ("cmp", b'0'),
            ("hp", b'5'),
            ("cmp2", b'3'),
            ("count", b'6')
        ]
    ),
    (
        4,
        37,
        "IfNumberOfCharacterHealthBars",
        [
            ("rg", b'3'),
            ("entity", b'5'),
            ("cmp", b'3'),
            ("bars", b'2'),
            ("cmp2", b'3'),
            ("count", b'6')
        ]
    ),
    (1000, 4, "EndUnconditionally", [("end", b'0')]),
    (
        1000,
        101,
        "GotoIfConditionGroupStateUncompiled",
        [("label", b'0'), ("state", b'0'), ("group", b'3')]
    ),
    (1000, 103, "GotoUnconditionally", [("label", b'0')]),
    (1001, 0, "WaitFixedTimeSeconds", [("seconds", b'6')]),
    (1001, 1, "WaitFixedTimeFrames", [("frames", b'5')]),
    (
        1003,
        0,
        "WaitForEventFlag",
        [("state", b'0'), ("type", b'0'), ("flag", b'5')]
    ),
    (
        1003,
        1,
        "SkipIfEventFlag",
        [
            ("lines", b'0'),
            ("state", b'0'),
            ("type", b'0'),
            ("flag", b'5')
        ]
    ),
    (
        1003,
        2,
        "EndIfEventFlag",
        [
            ("end", b'0'),
            ("state", b'0'),
            ("type", b'0'),
            ("flag", b'5')
        ]
    ),
    (
        1003,
        3,
        "SkipIfBatchEventFlags",
        [
            ("lines", b'0'),
            ("state", b'0'),
            ("type", b'0'),
            ("first", b'5'),
            ("last", b'5')
        ]
    ),
    (
        1003,
        101,
        "GotoIfEventFlag",
        [
            ("label", b'0'),
            ("state", b'0'),
            ("type", b'0'),
            ("flag", b'5')
        ]
    ),
    (1014, 0, "Label0", []),
    (1014, 1, "Label1", []),
    (1014, 2, "Label2", []),
    (1014, 3, "Label3", []),
    (1014, 4, "Label4", []),
    (1014, 5, "Label5", []),
    (1014, 6, "Label6", []),
    (
        2000,
        0,
        "InitializeEvent",
        [("slot", b'5'), ("event", b'2')]
    ),
    (2000, 6, "InitializeCommonEvent", [("event", b'2')]),
    (
        2002,
        4,
        "PlayCutsceneAndWarpPlayer",
        [
            ("cutscene", b'5'),
            ("method", b'2'),
            ("point", b'5'),
            ("area", b'0'),
            ("block", b'0'),
            ("player", b'5')
        ]
    ),
    (
        2002,
        13,
        "PlayCutsceneAndWarpPlayerWithLighting",
        [
            ("cutscene", b'5'),
            ("method", b'2'),
            ("point", b'5'),
            ("area", b'0'),
            ("block", b'0'),
            ("player", b'5'),
            ("time_of_day", b'0'),
            ("state", b'3')
        ]
    ),
    (2003, 2, "SetEventFlag", [("flag", b'5'), ("state", b'0')]),
    (
        2003,
        11,
        "DisplayBossHealthBar",
        [
            ("enabled", b'3'),
            ("entity", b'5'),
            ("slot", b'4'),
            ("name", b'5')
        ]
    ),
    (2003, 12, "HandleBossDefeat", [("entity", b'5')]),
    (
        2003,
        18,
        "ForceAnimationPlayback",
        [
            ("entity", b'5'),
            ("anim", b'5'),
            ("loop", b'0'),
            ("wait", b'0'),
            ("ignore_wait", b'0'),
            ("unk", b'0'),
            ("unk_f", b'6')
        ]
    ),
    (2003, 28, "AwardAchievement", [("achievement", b'5')]),
    (
        2003,
        62,
        "SetNetworkconnectedEventFlag",
        [("flag", b'5'), ("state", b'0')]
    ),
    (
        2003,
        63,
        "BatchSetNetworkconnectedEventFlags",
        [("first", b'5'), ("last", b'5'), ("state", b'0')]
    ),
    (2003, 80, "SetAreaEnvmap", [("envmap", b'0')]),
    (
        2003,
        81,
        "SetLighting",
        [("time_of_day", b'0'), ("unk", b'6')]
    ),
    (2003, 82, "SetMenuFade", [("fade", b'3'), ("seconds", b'6')]),
    (
        2003,
        88,
        "SetAreaGparamSubID",
        [
            ("area", b'2'),
            ("block", b'2'),
            ("sub_id", b'0'),
            ("seconds", b'6')
        ]
    ),
    (
        2004,
        1,
        "SetCharacterAIState",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2004,
        2,
        "SetCharacterTeamType",
        [("entity", b'5'), ("team", b'0')]
    ),
    (
        2004,
        5,
        "ChangeCharacterEnableState",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2004,
        6,
        "EzstateInstructionRequest",
        [("entity", b'5'), ("command", b'5'), ("slot", b'0')]
    ),
    (
        2004,
        8,
        "SetSpEffect",
        [("entity", b'5'), ("speffect", b'5')]
    ),
    (
        2004,
        12,
        "SetCharacterImmortality",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2004,
        13,
        "SetCharacterHome",
        [("entity", b'5'), ("area", b'5')]
    ),
    (
        2004,
        15,
        "SetCharacterInvincibility",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (2004, 20, "RequestCharacterAIReplan", [("entity", b'5')]),
    (
        2004,
        21,
        "ClearSpEffect",
        [("entity", b'5'), ("speffect", b'5')]
    ),
    (
        2004,
        26,
        "ChangeCharacterDispmask",
        [("entity", b'5'), ("bit", b'0'), ("switch", b'0')]
    ),
    (
        2004,
        29,
        "SetCharacterBackreadState",
        [("entity", b'5'), ("removed", b'0')]
    ),
    (
        2004,
        30,
        "SetCharacterHPBarDisplay",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2004,
        34,
        "SetNetworkUpdateRate",
        [("entity", b'5'), ("fixed", b'0'), ("frequency", b'3')]
    ),
    (
        2004,
        39,
        "SetCharacterAnimationState",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2004,
        41,
        "IssueShortWarpRequest",
        [
            ("entity", b'5'),
            ("warp_type", b'0'),
            ("destination", b'5'),
            ("dummy", b'5')
        ]
    ),
    (
        2004,
        65,
        "ForceCharacterTarget",
        [("targeter", b'5'), ("targeted", b'5')]
    ),
    (
        2005,
        1,
        "RequestObjectDestruction",
        [("entity", b'5'), ("slot", b'3')]
    ),
    (
        2005,
        3,
        "DeactivateObject",
        [("entity", b'5'), ("state", b'0')]
    ),
    (
        2006,
        1,
        "DeleteMapSFX",
        [("entity", b'5'), ("only_root", b'0')]
    ),
    (2006, 2, "SpawnMapSFX", [("entity", b'5')]),
    (2008, 4, "SetAreaCamerasetparamSubid", [("sub_id", b'5')]),
    (
        2010,
        1,
        "PlayBGM",
        [
            ("play", b'0'),
            ("slot", b'1'),
            ("entity", b'5'),
            ("sound_type", b'5'),
            ("sound", b'5')
        ]
    ),
    (
        2010,
        2,
        "PlaySE",
        [("entity", b'5'), ("sound_type", b'5'), ("sound", b'5')]
    ),
    (
        2010,
        3,
        "SetMapSoundState",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2010,
        4,
        "EnableBossMapSound",
        [("entity", b'5'), ("enabled", b'0')]
    ),
    (
        2010,
        6,
        "EnableMapSoundWithFade",
        [("entity", b'5'), ("enabled", b'0'), ("fade", b'6')]
    ),
    (
        2012,
        1,
        "ActivateMapPart",
        [("entity", b'5'), ("state", b'0')]
    ),
    (2012, 8, "SetAreaWelcomeMessageState", [("state", b'0')]),
];

pub fn def(bank: i32, id: i32) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.bank == bank && d.id == id)
}

/// Decodes the documented arguments of `ins`. Trailing words past the documented list (the
/// start arguments of `InitializeEvent`) are returned as `i32`s named `argN`.
pub fn decode(ins: &Instruction) -> Option<(&'static Def, Vec<(String, Arg)>)> {
    let d = def(ins.bank, ins.id)?;
    let mut out = Vec::new();
    let mut pos = 0usize;
    for &(name, code) in d.args {
        let t = ArgType::from_code(code);
        let size = t.size();
        pos = pos.div_ceil(size) * size;
        let b = ins.args.get(pos..pos + size)?;
        let v = match t {
            ArgType::U8 => Arg::Int(b[0] as i64),
            ArgType::I8 => Arg::Int(b[0] as i8 as i64),
            ArgType::U16 => Arg::Int(u16::from_le_bytes([b[0], b[1]]) as i64),
            ArgType::I16 => Arg::Int(i16::from_le_bytes([b[0], b[1]]) as i64),
            ArgType::U32 => Arg::Int(u32::from_le_bytes(b.try_into().unwrap()) as i64),
            ArgType::I32 => Arg::Int(i32::from_le_bytes(b.try_into().unwrap()) as i64),
            ArgType::F32 => Arg::Float(f32::from_le_bytes(b.try_into().unwrap())),
        };
        out.push((name.to_owned(), v));
        pos += size;
    }
    if matches!((ins.bank, ins.id), (2000, 0) | (2000, 6)) {
        let mut k = 0;
        while let Some(v) = ins.i32_at(pos) {
            out.push((format!("arg{k}"), Arg::Int(v as i64)));
            pos += 4;
            k += 1;
        }
    }
    Some((d, out))
}

/// `Name(a=1, b=2)` for a documented instruction, `bank[id](raw words)` otherwise.
pub fn format(ins: &Instruction) -> String {
    match decode(ins) {
        Some((d, args)) => {
            let list: Vec<String> = args.iter().map(|(n, v)| format!("{n}={v}")).collect();
            format!("{}({})", d.name, list.join(", "))
        }
        None => {
            let words: Vec<String> = ins.words().map(|w| w.to_string()).collect();
            format!("{}[{:02}]({})", ins.bank, ins.id, words.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(bank: i32, id: i32, args: Vec<u8>) -> Instruction {
        Instruction {
            bank,
            id,
            args,
            layer: None,
        }
    }

    #[test]
    fn packs_arguments_with_natural_alignment() {
        // DisplayBossHealthBar(enabled i8, entity i32, slot i16, name i32).
        let mut a = vec![1, 0, 0, 0];
        a.extend_from_slice(&1110850i32.to_le_bytes());
        a.extend_from_slice(&0i16.to_le_bytes());
        a.extend_from_slice(&[0, 0]);
        a.extend_from_slice(&905060i32.to_le_bytes());
        let (d, args) = decode(&ins(2003, 11, a)).unwrap();
        assert_eq!(d.name, "DisplayBossHealthBar");
        assert_eq!(args[1].1, Arg::Int(1110850));
        assert_eq!(args[3].1, Arg::Int(905060));
    }

    #[test]
    fn unknown_instructions_print_raw_words() {
        let s = format(&ins(9999, 1, 7i32.to_le_bytes().to_vec()));
        assert_eq!(s, "9999[01](7)");
    }
}
