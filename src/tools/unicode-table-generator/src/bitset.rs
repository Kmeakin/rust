use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::ops::Range;

use crate::{Bin, Lookup, writeln};

fn uniq<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v.dedup();
    v
}

pub fn emit_bitset(ranges: &[Range<u32>]) -> Result<Lookup, ()> {
    let first_code_point = ranges.first().unwrap().start;
    let last_code_point = ranges.last().unwrap().end;
    // bitset for every bit in the codepoint range
    let mut buckets = vec![0u64; (last_code_point.div_ceil(64)) as usize];
    for codepoint in ranges.iter().cloned().flatten() {
        let bucket = codepoint as usize / 64;
        let bit = codepoint as u64 % 64;
        buckets[bucket] |= 1 << bit;
    }

    // Ensure that there's a zero word in the dataset, used for padding and
    // such.
    buckets.push(0);
    let words = buckets;
    let unique_words = uniq(words.clone());
    if unique_words.len() > u8::MAX as usize {
        return Err(());
    }
    // needed for the chunk mapping to work
    assert_eq!(unique_words[0], 0, "has a zero word");
    let canonicalized = canonicalize(&unique_words);
    let word_indices = canonicalized.unique_mapping;
    let compressed_words = words.iter().map(|w| word_indices[w]).collect::<Vec<u8>>();

    let best = (1..=64)
        .map(|length| emit_chunk_map(word_indices[&0], &compressed_words, length))
        .min_by_key(|temp| temp.bytes_used)
        .unwrap();
    let Lookup { mut file, mut bytes_used, .. } = best;

    writeln!(
        file,
        "\
        static BITSET: [u64; {}] = {:?};
        static BITSET_MAPPED: [(u8, Function); {}] = {:?};",
        canonicalized.canonical_words.len(),
        canonicalized.canonical_words,
        canonicalized.canonicalized_words.len(),
        canonicalized.canonicalized_words,
    );
    bytes_used += size_of_val(canonicalized.canonical_words.as_slice());
    bytes_used += 2 * canonicalized.canonicalized_words.len();

    writeln!(
        file,
        "pub const fn lookup(c: char) -> bool {{
            debug_assert!(!c.is_ascii());
            (c as u32) >= {first_code_point:#04x} &&
                super::bitset_search(
                    c as u32,
                    &L1_LUT,
                    &L2_LUT,
                    &BITSET,
                    &BITSET_MAPPED,
                )
        }}"
    );

    Ok(Lookup { file, bytes_used, desc: "bitset" })
}

fn emit_chunk_map(zero_at: u8, compressed_words: &[u8], chunk_length: usize) -> Lookup {
    let mut compressed_words = compressed_words.to_vec();
    // pad out bitset index with zero words so we have all chunks of
    // chunkchunk_length
    compressed_words.resize(compressed_words.len().next_multiple_of(chunk_length), zero_at);

    let chunks = compressed_words.chunks(chunk_length).collect::<BTreeSet<_>>();
    let chunk_map = chunks
        .iter()
        .enumerate()
        .map(|(idx, &chunk)| (chunk, u8::try_from(idx).unwrap()))
        .collect::<HashMap<_, _>>();
    let chunk_indices =
        compressed_words.chunks(chunk_length).map(|chunk| chunk_map[chunk]).collect::<Vec<_>>();
    let chunks = chunks.into_iter().collect::<Vec<_>>();

    let file = format!(
        "
        use super::Function;

        static L1_LUT: [u8; {}] = {chunk_indices:?};
        static L2_LUT: [[u8; {chunk_length}]; {}] = {chunks:?};",
        chunk_indices.len(),
        chunks.len(),
    );

    let mut bytes_used = 0;
    bytes_used += size_of_val(chunk_indices.as_slice());
    bytes_used += chunk_length * chunks.len();

    Lookup { file, bytes_used, desc: "bitset" }
}

struct Canonicalized {
    canonical_words: Vec<Bin<u64>>,
    canonicalized_words: Vec<(u8, Function)>,

    /// Maps an input unique word to the associated index (u8) which is into
    /// canonical_words or canonicalized_words (in order).
    unique_mapping: HashMap<u64, u8>,
}

#[derive(Copy, Clone)]
enum Function {
    Rotate(u32),
    Invert,
    RotateAndInvert(u32),
    ShiftRight(u32),
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rotate(amount) => write!(f, "Function::rotate({amount})"),
            Self::Invert => write!(f, "Function::invert()"),
            Self::RotateAndInvert(amount) => write!(f, "Function::rotate_and_invert({amount})"),
            Self::ShiftRight(amount) => write!(f, "Function::shift_right({amount})"),
        }
    }
}

impl Function {
    fn find(to: u64, from: u64) -> Option<Self> {
        if to == from {
            return None;
        }

        // All possible distinct rotations
        for rotation in 1u32..64 {
            if to.rotate_right(rotation) == from {
                return Some(Function::Rotate(rotation));
            }
        }

        if !to == from {
            return Some(Function::Invert);
        }

        // All possible distinct rotations, inverted
        for rotation in 1u32..64 {
            if (!to.rotate_right(rotation)) == from {
                return Some(Function::RotateAndInvert(rotation));
            }
        }

        // All possible shifts
        for shift_by in 1u32..64 {
            if to == (from >> shift_by) {
                return Some(Function::ShiftRight(shift_by));
            }
        }

        None
    }
}

fn canonicalize(unique_words: &[u64]) -> Canonicalized {
    // key is the word being mapped to
    let mut mappings: BTreeMap<u64, Vec<(u64, Function)>> = BTreeMap::new();
    for &to in unique_words {
        for &from in unique_words {
            if let Some(fun) = Function::find(to, from) {
                mappings.entry(from).or_default().push((to, fun));
            }
        }
    }
    // These are the bitset words which will be represented "raw" (as a u64)
    let mut canonical_words = Vec::new();
    // These are mapped words, which will be represented by an index into
    // the canonical_words and a Mapping; u16 when encoded.
    let mut canonicalized_words = Vec::new();
    let mut unique_mapping = HashMap::new();

    #[derive(Debug, PartialEq, Eq)]
    enum UniqueMapping {
        Canonical(usize),
        Canonicalized(usize),
    }

    // Map 0 first, so that it is the first canonical word.
    // This is realistically not inefficient because 0 is not mapped to by
    // anything else (a shift pattern could do it, but would be wasteful).
    //
    // However, 0s are quite common in the overall dataset, and it is quite
    // wasteful to have to go through a mapping function to determine that
    // we have a zero.
    //
    // FIXME: Experiment with choosing most common words in overall data set
    // for canonical when possible.
    while let Some((&to, _)) = mappings
        .iter()
        .find(|&(&to, _)| to == 0)
        .or_else(|| mappings.iter().max_by_key(|m| m.1.len()))
    {
        // Get the mapping with the most entries. Currently, no mapping can
        // only exist transitively (i.e., there is no A, B, C such that A
        // does not map to C and but A maps to B maps to C), so this is
        // guaranteed to be acceptable.
        //
        // In the future, we may need a more sophisticated algorithm to
        // identify which keys to prefer as canonical.
        let mapped_from = mappings.remove(&to).unwrap();
        for (from, fun) in &mapped_from {
            // Remove the entries which mapped to this one.
            // Noting that it should be associated with the Nth canonical word.
            //
            // We do not assert that this is present, because there may be
            // no mappings to the `from` word; that's fine.
            mappings.remove(from);
            assert_eq!(
                unique_mapping
                    .insert(*from, UniqueMapping::Canonicalized(canonicalized_words.len())),
                None
            );
            canonicalized_words.push((u8::try_from(canonical_words.len()).unwrap(), *fun));

            // Remove the now-canonicalized word from other mappings,
            // to ensure that we deprioritize them in the next iteration of
            // the while loop.
            for mapped in mappings.values_mut() {
                let mut i = 0;
                while i != mapped.len() {
                    if mapped[i].0 != *from {
                        i += 1;
                        continue;
                    }
                    mapped.remove(i);
                }
            }
        }
        assert_eq!(
            unique_mapping.insert(to, UniqueMapping::Canonical(canonical_words.len())),
            None
        );
        canonical_words.push(to);

        // Remove the now-canonical word from other mappings, to ensure that
        // we deprioritize them in the next iteration of the while loop.
        for mapped in mappings.values_mut() {
            let mut i = 0;
            while i != mapped.len() {
                if mapped[i].0 != to {
                    i += 1;
                    continue;
                }
                mapped.remove(i);
            }
        }
    }

    // Any words which we couldn't shrink, just stick into the canonical
    // words.
    //
    // FIXME: work harder -- there are more possibilities for mapping
    // functions (e.g., multiplication, shifting instead of rotation, etc.)
    // We'll probably always have some slack though so this loop will still
    // be needed.
    for &w in unique_words {
        unique_mapping.entry(w).or_insert_with(|| {
            canonical_words.push(w);
            UniqueMapping::Canonical(canonical_words.len() - 1)
        });
    }
    assert_eq!(canonicalized_words.len() + canonical_words.len(), unique_words.len());
    assert_eq!(unique_mapping.len(), unique_words.len());

    let unique_mapping = unique_mapping
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                UniqueMapping::Canonicalized(idx) => {
                    u8::try_from(canonical_words.len() + idx).unwrap()
                }
                UniqueMapping::Canonical(idx) => u8::try_from(idx).unwrap(),
            };
            (key, value)
        })
        .collect::<HashMap<_, _>>();

    let mut distinct_indices = BTreeSet::new();
    for &w in unique_words {
        let idx = unique_mapping.get(&w).unwrap();
        assert!(distinct_indices.insert(idx));
    }

    let canonical_words = canonical_words.into_iter().map(Bin).collect();
    Canonicalized { unique_mapping, canonical_words, canonicalized_words }
}
