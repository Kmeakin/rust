//! Operations related to UTF-8 validation.

use super::Utf8Error;
use crate::mem;

/// Mask of the value bits of a continuation byte.
const CONT_MASK: u8 = 0x3F;

/// Checks whether the byte is a UTF-8 continuation byte (i.e., starts with the
/// bits `10`).
#[inline]
pub(super) const fn utf8_is_cont_byte(byte: u8) -> bool {
    (byte as i8) < -64
}

/// Reads the next code point out of a byte iterator (assuming a
/// UTF-8-like encoding).
///
/// # Safety
///
/// `bytes` must produce a valid UTF-8-like (UTF-8 or WTF-8) string
#[unstable(feature = "str_internals", issue = "none")]
#[inline]
pub unsafe fn next_code_point<'a, I>(bytes: &mut I) -> Option<u32>
where
    I: Iterator<Item = &'a u8>,
{
    let b0 = *bytes.next()?;
    if b0 < 0x80 {
        // 1 byte case:
        return Some(u32::from(b0));
    }

    // SAFETY: `bytes` is UTF8-like
    let b1 = unsafe { *bytes.next().unwrap_unchecked() };
    let b0_b1 = u32::from(b0 & 0x1F) << 6 | u32::from(b1 & CONT_MASK);
    if b0 < 0xE0 {
        // 2 byte case:
        // c = (b0 & 0x1F) << 6 | (b1 & 0x3F);
        let c = b0_b1;
        return Some(c);
    }

    // SAFETY: `bytes` is UTF8-like
    let b2 = unsafe { *bytes.next().unwrap_unchecked() };
    let b0_b1_b2 = b0_b1 << 6 | u32::from(b2 & CONT_MASK);
    if b0 < 0xF0 {
        // 3 byte case:
        // c = (b0 & 0x0F) << 12 | (b1 & 0x3F) << 6 | (b2 & 0x3F);
        let c = b0_b1_b2;
        return Some(c);
    }

    // SAFETY: `bytes` is UTF8-like
    let b3 = unsafe { *bytes.next().unwrap_unchecked() };
    let b0_b1_b2_b3 = b0_b1_b2 << 6 | u32::from(b3 & CONT_MASK);
    // 4 byte case:
    // c = (b0 & 0x07) << 18 | (b1 & 0x3F) << 12 | (b2 & 0x3F) << 6 | (b3 & 0x3F);
    let c = b0_b1_b2_b3 & 0x1F_FF_FF;
    Some(c)
}

/// Reads the last code point out of a byte iterator (assuming a
/// UTF-8-like encoding).
///
/// # Safety
///
/// `bytes` must produce a valid UTF-8-like (UTF-8 or WTF-8) string
#[unstable(feature = "str_internals", issue = "none")]
#[inline]
pub unsafe fn next_code_point_reverse<'a, I>(bytes: &mut I) -> Option<u32>
where
    I: DoubleEndedIterator<Item = &'a u8>,
{
    let b3 = *bytes.next_back()?;
    if b3 < 0x80 {
        return Some(u32::from(b3));
    }

    // SAFETY: `bytes` is UTF8-like
    let b2 = unsafe { *bytes.next_back().unwrap_unchecked() };
    let b2_b3 = u32::from(b2 & CONT_MASK) << 6 | u32::from(b3 & CONT_MASK);
    if !utf8_is_cont_byte(b2) {
        // 2 byte case:
        // c = (b2 & 0x1F) << 6 | (b3 & 0x3F);
        let c = b2_b3;
        return Some(c);
    }

    // SAFETY: `bytes` is UTF8-like
    let b1 = unsafe { *bytes.next_back().unwrap_unchecked() };
    let b1_b2_b3 = u32::from(b1 & CONT_MASK) << 12 | b2_b3;
    if !utf8_is_cont_byte(b1) {
        // 3 byte case:
        // c = (b1 & 0x0F) << 12 | (b2 & 0x3F) << 6 | (b3 & 0x3F);
        let c = b1_b2_b3 & 0xFFFF;
        return Some(c);
    }

    // SAFETY: `bytes` is UTF8-like
    let b0 = unsafe { *bytes.next_back().unwrap_unchecked() };

    // 4 byte case:
    // c = (b0 & 0x07) << 18 | (b1 & 0x3F) << 12 | (b2 & 0x3F) << 6 | (b3 & 0x3F);
    let c = u32::from(b0 & 0x07) << 18 | b1_b2_b3;
    Some(c)
}

const NONASCII_MASK: usize = usize::repeat_u8(0x80);

/// Returns `true` if any byte in the word `x` is nonascii (>= 128).
#[inline]
const fn contains_nonascii(x: usize) -> bool {
    (x & NONASCII_MASK) != 0
}

/// Walks through `v` checking that it's a valid UTF-8 sequence,
/// returning `Ok(())` in that case, or, if it is invalid, `Err(err)`.
#[inline(always)]
#[rustc_const_unstable(feature = "str_internals", issue = "none")]
pub(super) const fn run_utf8_validation(v: &[u8]) -> Result<(), Utf8Error> {
    let mut index = 0;
    let len = v.len();

    let usize_bytes = mem::size_of::<usize>();
    let ascii_block_size = 2 * usize_bytes;
    let blocks_end = if len >= ascii_block_size { len - ascii_block_size + 1 } else { 0 };
    let align = v.as_ptr().align_offset(usize_bytes);

    while index < len {
        let old_offset = index;
        macro_rules! err {
            ($error_len: expr) => {
                return Err(Utf8Error { valid_up_to: old_offset, error_len: $error_len })
            };
        }

        macro_rules! next {
            () => {{
                index += 1;
                // we needed data, but there was none: error!
                if index >= len {
                    err!(None)
                }
                v[index]
            }};
        }

        let first = v[index];
        if first >= 128 {
            let w = utf8_char_width(first);
            // 2-byte encoding is for codepoints  \u{0080} to  \u{07ff}
            //        first  C2 80        last DF BF
            // 3-byte encoding is for codepoints  \u{0800} to  \u{ffff}
            //        first  E0 A0 80     last EF BF BF
            //   excluding surrogates codepoints  \u{d800} to  \u{dfff}
            //               ED A0 80 to       ED BF BF
            // 4-byte encoding is for codepoints \u{10000} to \u{10ffff}
            //        first  F0 90 80 80  last F4 8F BF BF
            //
            // Use the UTF-8 syntax from the RFC
            //
            // https://tools.ietf.org/html/rfc3629
            // UTF8-1      = %x00-7F
            // UTF8-2      = %xC2-DF UTF8-tail
            // UTF8-3      = %xE0 %xA0-BF UTF8-tail / %xE1-EC 2( UTF8-tail ) /
            //               %xED %x80-9F UTF8-tail / %xEE-EF 2( UTF8-tail )
            // UTF8-4      = %xF0 %x90-BF 2( UTF8-tail ) / %xF1-F3 3( UTF8-tail ) /
            //               %xF4 %x80-8F 2( UTF8-tail )
            match w {
                2 => {
                    if next!() as i8 >= -64 {
                        err!(Some(1))
                    }
                }
                3 => {
                    match (first, next!()) {
                        (0xE0, 0xA0..=0xBF)
                        | (0xE1..=0xEC, 0x80..=0xBF)
                        | (0xED, 0x80..=0x9F)
                        | (0xEE..=0xEF, 0x80..=0xBF) => {}
                        _ => err!(Some(1)),
                    }
                    if next!() as i8 >= -64 {
                        err!(Some(2))
                    }
                }
                4 => {
                    match (first, next!()) {
                        (0xF0, 0x90..=0xBF) | (0xF1..=0xF3, 0x80..=0xBF) | (0xF4, 0x80..=0x8F) => {}
                        _ => err!(Some(1)),
                    }
                    if next!() as i8 >= -64 {
                        err!(Some(2))
                    }
                    if next!() as i8 >= -64 {
                        err!(Some(3))
                    }
                }
                _ => err!(Some(1)),
            }
            index += 1;
        } else {
            // Ascii case, try to skip forward quickly.
            // When the pointer is aligned, read 2 words of data per iteration
            // until we find a word containing a non-ascii byte.
            if align != usize::MAX && align.wrapping_sub(index) % usize_bytes == 0 {
                let ptr = v.as_ptr();
                while index < blocks_end {
                    // SAFETY: since `align - index` and `ascii_block_size` are
                    // multiples of `usize_bytes`, `block = ptr.add(index)` is
                    // always aligned with a `usize` so it's safe to dereference
                    // both `block` and `block.add(1)`.
                    unsafe {
                        let block = ptr.add(index) as *const usize;
                        // break if there is a nonascii byte
                        let zu = contains_nonascii(*block);
                        let zv = contains_nonascii(*block.add(1));
                        if zu || zv {
                            break;
                        }
                    }
                    index += ascii_block_size;
                }
                // step from the point where the wordwise loop stopped
                while index < len && v[index] < 128 {
                    index += 1;
                }
            } else {
                index += 1;
            }
        }
    }

    Ok(())
}

// https://tools.ietf.org/html/rfc3629
const UTF8_CHAR_WIDTH: &[u8; 256] = &[
    // 1  2  3  4  5  6  7  8  9  A  B  C  D  E  F
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 1
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 2
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 3
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 4
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 5
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 6
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 7
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 8
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 9
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // A
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // B
    0, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // C
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // D
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, // E
    4, 4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // F
];

/// Given a first byte, determines how many bytes are in this UTF-8 character.
#[unstable(feature = "str_internals", issue = "none")]
#[must_use]
#[inline]
pub const fn utf8_char_width(b: u8) -> usize {
    UTF8_CHAR_WIDTH[b as usize] as usize
}
