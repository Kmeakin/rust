use std::ops::Range;

use crate::{Hex, Lookup, writeln};

pub fn emit_cascading_map(ranges: &[Range<u32>]) -> Lookup {
    let mut map: [u8; 256] = [0; 256];

    let points =
        ranges.iter().cloned().flatten().map(|c| u16::try_from(c).unwrap()).collect::<Vec<_>>();
    let chunks = points
        .chunk_by(|c1, c2| {
            let [high_byte1, _] = c1.to_be_bytes();
            let [high_byte2, _] = c2.to_be_bytes();
            high_byte1 == high_byte2
        })
        .map(|chunk| {
            let c = chunk.first().unwrap();
            let [high_byte, _] = c.to_be_bytes();
            (Hex(high_byte), chunk)
        });

    let mut bit_for_high_byte = 1u8;
    let mut arms = String::new();

    for (high_byte, chunk) in chunks {
        if let [codepoint] = chunk {
            let codepoint = Hex(*codepoint);
            writeln!(arms, "{high_byte} => c as u32 == {codepoint},");
            continue;
        }

        for codepoint in chunk {
            map[(*codepoint & 0xff) as usize] |= bit_for_high_byte;
        }
        writeln!(
            arms,
            "{high_byte} => WHITESPACE_MAP[c as usize & 0xff] & {bit_for_high_byte} != 0,"
        );
        bit_for_high_byte <<= 1;
    }

    let bytes_used = size_of_val(&map);
    let file = format!(
        "static WHITESPACE_MAP: [u8; 256] = {map:?};

        #[inline]
        pub const fn lookup(c: char) -> bool {{
            debug_assert!(!c.is_ascii());
            match c as u32 >> 8 {{
                {arms}\
                _ => false,
            }}
        }}"
    );

    Lookup { file, bytes_used, desc: "cascading" }
}
