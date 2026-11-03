//! EMEVD counters stored in consecutive event-flag bits, least significant bit first.

use std::collections::HashSet;

pub fn read(flags: &HashSet<i32>, start: i32, bits: u8) -> u32 {
    (0..bits.min(32)).fold(0, |value, bit| {
        value | (u32::from(flags.contains(&(start + i32::from(bit)))) << bit)
    })
}

pub fn comparison(instruction: &sekiro_formats::emevd::Instruction, flags: &HashSet<i32>) -> bool {
    let start = instruction.i32_at(4).unwrap_or(0);
    let bits = instruction.u8_at(8).unwrap_or(0);
    let comparison = instruction.u8_at(9).unwrap_or(0);
    let rhs = instruction.i32_at(12).unwrap_or(0) as i64;
    let lhs = i64::from(read(flags, start, bits));
    match comparison {
        0 => lhs == rhs,
        1 => lhs != rhs,
        2 => lhs > rhs,
        3 => lhs < rhs,
        4 => lhs >= rhs,
        5 => lhs <= rhs,
        _ => false,
    }
}
