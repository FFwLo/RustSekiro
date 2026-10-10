//! The game's event scripts (extracted/event/*.emevd, the EVD layout tools/boss_events.py reads;
//! instruction meanings from DarkScript3's sekiro-common.emedf.json) for the rules the combat code
//! takes from them.

use std::collections::HashMap;

struct Instr {
    bank: i32,
    id: i32,
    args: Vec<u8>,
}

/// One event: its instructions and parameter substitutions (instruction, target byte, source
/// byte in the initializer's arguments, length).
struct Event {
    instrs: Vec<Instr>,
    params: Vec<(usize, usize, usize, usize)>,
}

fn parse(d: &[u8]) -> Option<HashMap<i64, Event>> {
    if d.get(..4)? != b"EVD\0" {
        return None;
    }
    let q = |o: usize| -> Option<i64> { Some(i64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?)) };
    let l = |o: usize| -> Option<i32> { Some(i32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?)) };
    let h: Vec<i64> = (0..16).map(|i| q(0x10 + i * 8)).collect::<Option<_>>()?;
    let (ev_n, ev_o, ins_o, par_o, arg_o) = (h[0] as usize, h[1] as usize, h[3] as usize, h[9] as usize, h[13] as usize);
    let mut out = HashMap::new();
    for i in 0..ev_n {
        let e = ev_o + i * 0x30;
        let (id, icount, io, pcount, po) = (q(e)?, q(e + 8)? as usize, q(e + 16)? as usize, q(e + 24)? as usize, q(e + 32)? as usize);
        let mut instrs = Vec::with_capacity(icount);
        for k in 0..icount {
            let o = ins_o + io + k * 0x20;
            // No arguments: the offset may be -1.
            let (alen, aoff) = (q(o + 8)? as usize, q(o + 16)?);
            let args = if alen == 0 {
                Vec::new()
            } else {
                let s = arg_o.checked_add(usize::try_from(aoff).ok()?)?;
                d.get(s..s.checked_add(alen)?)?.to_vec()
            };
            instrs.push(Instr { bank: l(o)?, id: l(o + 4)?, args });
        }
        let mut params = Vec::with_capacity(pcount);
        for k in 0..pcount {
            let o = par_o + po + k * 0x20;
            params.push((q(o)? as usize, q(o + 8)? as usize, q(o + 16)? as usize, l(o + 24)? as usize));
        }
        out.insert(id, Event { instrs, params });
    }
    Some(out)
}

fn int(a: &[u8], o: usize) -> Option<i64> {
    Some(i32::from_le_bytes(a.get(o..o + 4)?.try_into().ok()?) as i64)
}

/// A combat art's "has Spirit Emblems" SpEffect: on while the player (entity 10000) has one of
/// `residents` and holds at least `emblems` Spirit Emblems.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtGate {
    pub residents: Vec<i64>,
    pub gate: i64,
    pub emblems: u32,
}

/// common.emevd: event 9900 stores the goods 1000 + 1001 held (Spirit Emblems) in event value
/// 9910 every frame (2003[42] Store Item Amount Held, 2003[41] add); events 9930-9934, started
/// by event 0 with the cost as their argument, wait for "IF Character Has SpEffect 10000 <resident>"
/// (4[05], an OR group where two upgrade levels share one) and "IF Event Value 9910 >= <cost>"
/// (3[12], comparison 4), Set SpEffect <gate> (2004[08]), then wait for that to end and Clear it
/// (2004[21]): 140300 -> 140310 (2), 140501 -> 140510 (1), 140600 / 140601 -> 140610 (3),
/// 100286 -> 140410 (3), 140900 / 140901 -> 140910 (2). Read generically from the file.
fn art_gates_of(ev: &HashMap<i64, Event>) -> Vec<ArtGate> {
    const PLAYER: i64 = 10000;
    const GOODS: i64 = 3;
    const SPIRIT_EMBLEM: i64 = 1000;
    const GREATER_OR_EQUAL: u8 = 4;
    // The event values that hold the Spirit Emblems (Store Item Amount Held, goods 1000).
    let values: Vec<i64> = ev
        .values()
        .flat_map(|e| &e.instrs)
        .filter(|i| i.bank == 2003 && i.id == 42 && int(&i.args, 0) == Some(GOODS) && int(&i.args, 4) == Some(SPIRIT_EMBLEM))
        .filter_map(|i| int(&i.args, 8))
        .collect();
    let Some(init) = ev.get(&0) else { return Vec::new() };
    let mut out = Vec::new();
    for start in init.instrs.iter().filter(|i| i.bank == 2000 && i.id == 0) {
        let Some(e) = int(&start.args, 4).and_then(|id| ev.get(&id)) else { continue };
        let mut residents: Vec<i64> = e
            .instrs
            .iter()
            .filter(|i| i.bank == 4 && i.id == 5 && int(&i.args, 4) == Some(PLAYER) && i.args.get(12) == Some(&1))
            .filter_map(|i| int(&i.args, 8))
            .collect();
        // The set and the clear half each list them.
        residents.sort();
        residents.dedup();
        let gate = e.instrs.iter().find(|i| i.bank == 2004 && i.id == 8 && int(&i.args, 0) == Some(PLAYER)).and_then(|i| int(&i.args, 4));
        // The emblem check and its threshold, which comes from the start's argument when
        // substituted (argument bytes after the slot and the event id).
        let threshold = e.instrs.iter().enumerate().find_map(|(k, i)| {
            if i.bank != 3 || i.id != 12 || !int(&i.args, 4).is_some_and(|v| values.contains(&v)) || i.args.get(9) != Some(&GREATER_OR_EQUAL) {
                return None;
            }
            match e.params.iter().find(|p| p.0 == k && p.1 == 12 && p.3 == 4) {
                Some(p) => int(&start.args, 8 + p.2),
                None => int(&i.args, 12),
            }
        });
        if let (false, Some(gate), Some(t)) = (residents.is_empty(), gate, threshold) {
            out.push(ArtGate { residents, gate, emblems: t.max(0) as u32 });
        }
    }
    out
}

/// The art gates of extracted/event/common.emevd (None: the file is missing or unreadable).
pub fn art_gates() -> Option<&'static [ArtGate]> {
    static GATES: std::sync::OnceLock<Option<Vec<ArtGate>>> = std::sync::OnceLock::new();
    GATES
        .get_or_init(|| {
            let d = std::fs::read(crate::paths::root().join("extracted/event/common.emevd")).ok()?;
            Some(art_gates_of(&parse(&d)?))
        })
        .as_deref()
}

#[cfg(test)]
mod tests {
    #[test]
    fn common_emevd_art_gates() {
        let Some(g) = super::art_gates() else { return };
        let find = |r: i64| g.iter().find(|a| a.residents.contains(&r)).map(|a| (a.gate, a.emblems));
        assert_eq!(find(140300), Some((140310, 2)));
        assert_eq!(find(140501), Some((140510, 1)));
        assert_eq!(find(140601), Some((140610, 3)));
        assert_eq!(find(100286), Some((140410, 3)));
        assert_eq!(find(140901), Some((140910, 2)));
        // Ashina Cross (5500, resident 140400) has none.
        assert_eq!(find(140400), None);
    }
}
