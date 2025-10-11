use std::char;
use std::collections::BTreeMap;

use crate::{CharEscape, UnicodeData, fmt_list};

const INDEX_MASK: u32 = 1 << 22;

pub(crate) fn generate_case_mapping(data: &UnicodeData) -> (String, [usize; 2]) {
    let (lower_tables, lower_size) = generate_tables("LOWER", &data.to_lower);
    let (upper_tables, upper_size) = generate_tables("UPPER", &data.to_upper);
    let file = format!(
        "{HEADER}
        {lower_tables}
        {upper_tables}"
    );
    (file, [lower_size, upper_size])
}

fn generate_tables(case: &str, data: &BTreeMap<u32, [u32; 3]>) -> (String, usize) {
    let mut mappings = Vec::with_capacity(data.len());
    let mut multis = Vec::new();

    for (&key, &chars) in data.iter() {
        let key = char::from_u32(key).unwrap();

        if key.is_ascii() {
            continue;
        }

        let value = match chars {
            [c1, 0, 0] => c1,
            _ => {
                let chars = chars.map(|c| CharEscape(char::from_u32(c).unwrap()));
                multis.push(chars);
                INDEX_MASK | (u32::try_from(multis.len()).unwrap() - 1)
            }
        };

        mappings.push((CharEscape(key), value));
    }

    let size = size_of_val(mappings.as_slice()) + size_of_val(multis.as_slice());
    let file = format!(
        "
    #[rustfmt::skip]\nstatic {case}CASE_TABLE: &[(char, u32); {}] = &[{}];
    #[rustfmt::skip]\nstatic {case}CASE_TABLE_MULTI: &[[char; 3]; {}] = &[{}];",
        mappings.len(),
        fmt_list(mappings),
        multis.len(),
        fmt_list(multis),
    );

    (file, size)
}

static HEADER: &str = r"
const INDEX_MASK: u32 = 1 << 22;

pub fn to_lower(c: char) -> [char; 3] {
    if c.is_ascii() {
        return [c.to_ascii_lowercase(), '\0', '\0'];
    }

    let Ok(i) = LOWERCASE_TABLE.binary_search_by(|&(key, _)| key.cmp(&c)) else {
        return [c, '\0', '\0'];
    };

    let (_, u) = LOWERCASE_TABLE[i];
    match char::from_u32(u) {
        Some(c) => [c, '\0', '\0'],
        None => {
            // SAFETY: Index comes from statically generated table
            unsafe { *LOWERCASE_TABLE_MULTI.get_unchecked((u & (INDEX_MASK - 1)) as usize) }
        }
    }
}

pub fn to_upper(c: char) -> [char; 3] {
    if c.is_ascii() {
        return [c.to_ascii_uppercase(), '\0', '\0'];
    }

    let Ok(i) = UPPERCASE_TABLE.binary_search_by(|&(key, _)| key.cmp(&c)) else {
        return [c, '\0', '\0'];
    };

    let (_, u) = UPPERCASE_TABLE[i];
    match char::from_u32(u) {
        Some(c) => [c, '\0', '\0'],
        None => {
            // SAFETY: Index comes from statically generated table
            unsafe { *UPPERCASE_TABLE_MULTI.get_unchecked((u & (INDEX_MASK - 1)) as usize) }
        }
    }
}
";
