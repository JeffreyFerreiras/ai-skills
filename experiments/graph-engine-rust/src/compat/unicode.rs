use super::PyString;
use std::{collections::BTreeMap, sync::OnceLock};

include!(concat!(env!("OUT_DIR"), "/compat_provenance.rs"));
static TABLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/unicode-tables.bin"));
struct Unicode {
    flags_offset: usize,
    mappings: BTreeMap<u32, (Vec<u32>, Vec<u32>)>,
}
static DATA: OnceLock<Unicode> = OnceLock::new();

fn u32_at(offset: usize) -> u32 {
    u32::from_le_bytes(TABLE[offset..offset + 4].try_into().unwrap())
}
fn tables() -> &'static Unicode {
    DATA.get_or_init(|| {
        let flags_offset = 12 + u32_at(8) as usize;
        let count_offset = flags_offset + 0x110000 * 2;
        let count = u32_at(count_offset);
        let mut offset = count_offset + 4;
        let mut mappings = BTreeMap::new();
        for _ in 0..count {
            let point = u32_at(offset);
            let lower_length = usize::from(TABLE[offset + 4]);
            let folded_length = usize::from(TABLE[offset + 5]);
            offset += 6;
            let lower = (0..lower_length)
                .map(|index| u32_at(offset + index * 4))
                .collect();
            offset += lower_length * 4;
            let folded = (0..folded_length)
                .map(|index| u32_at(offset + index * 4))
                .collect();
            offset += folded_length * 4;
            mappings.insert(point, (lower, folded));
        }
        Unicode {
            flags_offset,
            mappings,
        }
    })
}
pub fn property(point: u32, bit: u8) -> bool {
    if point > 0x10ffff {
        return false;
    }
    let index = tables().flags_offset + point as usize * 2;
    u16::from_le_bytes(TABLE[index..index + 2].try_into().unwrap()) & (1 << bit) != 0
}
pub fn whitespace(point: u32) -> bool {
    property(point, 0)
}
pub fn decimal(point: u32) -> Option<u32> {
    if !property(point, 3) {
        return None;
    }
    let mut first = point;
    while first > 0 && property(first - 1, 3) {
        first -= 1;
    }
    Some((point - first) % 10)
}
pub fn lower(text: &PyString) -> PyString {
    let mut result = Vec::with_capacity(text.len());
    for (index, &point) in text.0.iter().enumerate() {
        if point == 0x3a3 {
            let preceding = text.0[..index]
                .iter()
                .rev()
                .find(|&&p| !property(p, 12))
                .is_some_and(|&p| property(p, 11));
            let following = text.0[index + 1..]
                .iter()
                .find(|&&p| !property(p, 12))
                .is_some_and(|&p| property(p, 11));
            result.push(if preceding && !following {
                0x3c2
            } else {
                0x3c3
            });
        } else if let Some((mapping, _)) = tables().mappings.get(&point) {
            result.extend(mapping);
        } else {
            result.push(point);
        }
    }
    PyString(result)
}
pub fn casefold(text: &PyString) -> PyString {
    PyString(
        text.0
            .iter()
            .flat_map(|p| {
                tables()
                    .mappings
                    .get(p)
                    .map(|(_, mapping)| mapping.clone())
                    .unwrap_or_else(|| vec![*p])
            })
            .collect(),
    )
}
pub fn normcase(text: &PyString, windows: bool) -> PyString {
    if !windows {
        return text.clone();
    }
    lower(&PyString(
        text.0
            .iter()
            .map(|&p| if p == 47 { 92 } else { p })
            .collect(),
    ))
}
