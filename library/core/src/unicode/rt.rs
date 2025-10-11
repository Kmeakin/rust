//! Runtime support for `unicode_data`.

#[repr(transparent)]
pub struct Function(u8);

impl Function {
    pub const fn invert() -> Self {
        Self(1 << 6)
    }

    pub const fn rotate(shift: u8) -> Self {
        assert!(shift < 64);
        Self(shift)
    }

    pub const fn rotate_and_invert(shift: u8) -> Self {
        assert!(shift < 64);
        Self((1 << 6) | shift)
    }

    pub const fn shift_right(shift: u8) -> Self {
        assert!(shift < 64);
        Self((1 << 7) | shift)
    }
}

// FIXME(const-hack): Revert to `slice::get` when slice indexing becomes possible in const.
const fn get<T: Copy>(slice: &[T], idx: usize) -> Option<T> {
    if idx < slice.len() { Some(slice[idx]) } else { None }
}

#[inline(always)]
pub const fn bitset_search<
    const L1_LEN: usize,
    const L2_LEN: usize,
    const L2_INNER_LEN: usize,
    const BITSET_LEN: usize,
    const BITSET_MAPPED_LEN: usize,
>(
    needle: u32,
    l1_lut: &[u8; L1_LEN],
    l2_lut: &[[u8; L2_INNER_LEN]; L2_LEN],
    bitset: &[u64; BITSET_LEN],
    bitset_mapped: &[(u8, Function); BITSET_MAPPED_LEN],
) -> bool {
    let bucket_idx = (needle / 64) as usize;
    let l1_idx = bucket_idx / L2_INNER_LEN;
    let chunk_piece = bucket_idx % L2_INNER_LEN;
    let Some(l2_idx) = get(l1_lut, l1_idx) else { return false };
    let bitset_idx = l2_lut[l2_idx as usize][chunk_piece] as usize;
    let word = if let Some(word) = get(bitset, bitset_idx) {
        word
    } else {
        const LOWER_6: u8 = (1 << 6) - 1;

        let (bitset_idx, Function(fun)) = bitset_mapped[bitset_idx - bitset.len()];
        let mut word = bitset[bitset_idx as usize];
        let should_invert = fun & (1 << 6) != 0;
        if should_invert {
            word = !word;
        }
        // Lower 6 bits
        let quantity = fun & LOWER_6;
        if fun & (1 << 7) != 0 {
            // shift
            word >>= quantity as u64;
        } else {
            word = word.rotate_left(quantity as u32);
        }
        word
    };
    (word & (1 << (needle % 64) as u64)) != 0
}

#[repr(transparent)]
pub struct ShortOffsetRunHeader(pub u32);

impl ShortOffsetRunHeader {
    pub const fn new(start_index: usize, prefix_sum: u32) -> Self {
        assert!(start_index < (1 << 11));
        assert!(prefix_sum < (1 << 21));

        Self((start_index as u32) << 21 | prefix_sum)
    }

    #[inline]
    pub const fn start_index(&self) -> usize {
        (self.0 >> 21) as usize
    }

    #[inline]
    pub const fn prefix_sum(&self) -> u32 {
        self.0 & ((1 << 21) - 1)
    }
}

/// # Safety
///
/// - The last element of `short_offset_runs` must be greater than `std::char::MAX`.
/// - The start indices of all elements in `short_offset_runs` must be less than `OFFSETS`.
#[inline(always)]
pub unsafe fn skip_search<const SOR: usize, const OFFSETS: usize>(
    needle: char,
    short_offset_runs: &[ShortOffsetRunHeader; SOR],
    offsets: &[u8; OFFSETS],
) -> bool {
    let needle = needle as u32;

    let last_idx =
        match short_offset_runs.binary_search_by_key(&(needle << 11), |header| header.0 << 11) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };
    // SAFETY: `last_idx` *cannot* be past the end of the array, as the last
    // element is greater than `std::char::MAX` (the largest possible needle)
    // as guaranteed by the caller.
    //
    // So, we cannot have found it (i.e. `Ok(idx) => idx + 1 != length`) and the
    // correct location cannot be past it, so `Err(idx) => idx != length` either.
    //
    // This means that we can avoid bounds checking for the accesses below, too.
    //
    // We need to use `intrinsics::assume` since the `panic_nounwind` contained
    // in `hint::assert_unchecked` may not be optimized out.
    unsafe { crate::intrinsics::assume(last_idx < SOR) };

    let mut offset_idx = short_offset_runs[last_idx].start_index();
    let length = if let Some(next) = short_offset_runs.get(last_idx + 1) {
        (*next).start_index() - offset_idx
    } else {
        offsets.len() - offset_idx
    };

    let prev =
        last_idx.checked_sub(1).map(|prev| short_offset_runs[prev].prefix_sum()).unwrap_or(0);

    let total = needle - prev;
    let mut prefix_sum = 0;
    for _ in 0..(length - 1) {
        // SAFETY: It is guaranteed that `length <= OFFSETS - offset_idx`,
        // so it follows that `length - 1 + offset_idx < OFFSETS`, therefore
        // `offset_idx < OFFSETS` is always true in this loop.
        //
        // We need to use `intrinsics::assume` since the `panic_nounwind` contained
        // in `hint::assert_unchecked` may not be optimized out.
        unsafe { crate::intrinsics::assume(offset_idx < OFFSETS) };
        let offset = offsets[offset_idx];
        prefix_sum += offset as u32;
        if prefix_sum > total {
            break;
        }
        offset_idx += 1;
    }
    offset_idx % 2 == 1
}
