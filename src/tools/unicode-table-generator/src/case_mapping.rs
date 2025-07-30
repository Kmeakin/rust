use std::char;
use std::fmt::{self, Write};

use crate::{CaseMap, UnicodeData, fmt_list};

pub(crate) fn generate_case_mapping(data: &UnicodeData) -> Result<String, fmt::Error> {
    let mut file = String::new();

    file.push_str(HEADER.trim_start());
    file.push('\n');
    file.push_str(&generate_tables("LOWER", &data.to_lower)?);
    file.push_str("\n\n");
    file.push_str(&generate_tables("UPPER", &data.to_upper)?);
    Ok(file)
}

#[derive(Default, Clone)]
struct Plane {
    singles: Vec<(u16, i16)>,
    single_keys: Vec<(HexEscape, u8, u8)>,
    single_vals: Vec<i16>,
    multi_keys: Vec<HexEscape>,
    multi_vals: Vec<[HexEscape; 3]>,
}

fn decompose(c: char) -> (u16, u16) {
    let plane = (c as u32) / 0x10_000;
    let code = (c as u32) % 0x10_000;
    assert!(plane <= 16);
    (plane as u16, code as u16)
}

fn generate_tables(case: &str, data: &CaseMap) -> Result<String, fmt::Error> {
    let mut planes = vec![Plane::default(); 17];
    let mut num_planes = 0;
    for (&key, mapped) in data.iter() {
        if key.is_ascii() {
            continue;
        }

        let (key_plane, key_code) = decompose(key);
        let plane = &mut planes[key_plane as usize];
        match mapped {
            &[val, '\0', '\0'] => {
                let (val_plane, val_code) = decompose(val);
                assert_eq!(key_plane, val_plane);
                let delta = val_code.wrapping_sub(key_code) as i16;
                plane.singles.push((key_code, delta));
            }
            &chars => {
                plane.multi_keys.push(HexEscape(key_code));
                plane.multi_vals.push(chars.map(|val| {
                    let (val_plane, val_code) = decompose(val);
                    assert_eq!(key_plane, val_plane);
                    HexEscape(val_code)
                }));
            }
        }

        num_planes = num_planes.max(key_plane + 1);
    }
    planes.truncate(num_planes as usize);

    for plane in &mut planes {
        plane.singles.sort_unstable_by_key(|&(key, _)| key);

        let singles: Vec<_> = plane
            .singles
            .chunk_by(|(key1, val1), (key2, val2)| val1 == val2 && *key1 == key2 - 1)
            .map(|chunk| {
                let (start, val) = chunk.first().unwrap();
                let (end, _) = chunk.last().unwrap();
                (*start, *end, *val)
            })
            .collect();

        let singles = singles
            .chunk_by(|(start1, end1, val1), (start2, end2, val2)| {
                val1 == val2 && start1 == end1 && start2 == end2 && *start1 == start2 - 2
            })
            .map(|chunk| {
                let (start, _, val) = chunk.first().unwrap();
                let (_, end, _) = chunk.last().unwrap();
                let len = u8::try_from(end - start).unwrap();
                let step = if chunk.len() == 1 { 1u8 } else { 2u8 };
                let key = (HexEscape(*start), len, step);
                (key, *val)
            });

        let (keys, vals): (Vec<_>, Vec<_>) = singles.unzip();
        plane.single_keys = keys;
        plane.single_vals = vals;
    }

    let mut tables = String::new();
    writeln!(tables, "static {case}CASE_TABLES: &[Plane; {}] = &[", num_planes)?;
    for plane in planes {
        writeln!(tables, "    Plane {{")?;

        writeln!(
            tables,
            "        // {} entries, {} bytes",
            plane.single_keys.len(),
            size_of_val(&plane.single_keys[..])
        )?;
        writeln!(tables, "        single_keys: &[{}],", fmt_list(plane.single_keys))?;

        writeln!(
            tables,
            "        // {} entries, {} bytes",
            plane.single_vals.len(),
            size_of_val(&plane.single_vals[..])
        )?;
        writeln!(tables, "        single_vals: &[{}],", fmt_list(plane.single_vals))?;

        writeln!(
            tables,
            "        // {} entries, {} bytes",
            plane.multi_vals.len(),
            size_of_val(&plane.multi_keys[..])
        )?;
        writeln!(tables, "        multi_keys: &[{}],", fmt_list(plane.multi_keys))?;

        writeln!(
            tables,
            "        // {} entries, {} bytes",
            plane.multi_vals.len(),
            size_of_val(&plane.multi_vals[..])
        )?;
        writeln!(tables, "        multi_vals: &[{}],", fmt_list(plane.multi_vals))?;

        writeln!(tables, "    }},")?;
    }
    writeln!(tables, "];")?;

    Ok(tables)
}

#[derive(Default, Clone)]
struct HexEscape(u16);

impl fmt::Debug for HexEscape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#06x?}", self.0)
    }
}

static HEADER: &str = r"
struct Plane {
    single_keys: &'static [(u16, u8, u8)],
    single_vals: &'static [i16],
    multi_keys: &'static [u16],
    multi_vals: &'static [[char; 3]],
}

#[inline]
fn lookup(c: char, tables: &[Plane]) -> [char; 3] {
    let plane_index = (c as u32) / 0x10_000;
    let code = ((c as u32) % 0x10_000) as u16;

    let Some(plane) = tables.get(plane_index as usize) else {
        return [c, '\0', '\0'];
    };

    if let Ok(multi_index) = plane.multi_keys.binary_search(&code) {
        // SAFETY: Index comes from statically generated table
        return *unsafe { plane.multi_vals.get_unchecked(multi_index) };
    }

    if let Ok(single_index) = plane.single_keys.binary_search_by(|(start, len, _)| {
        let end = start + *len as u16;
        if *start <= code && code <= end {
            crate::cmp::Ordering::Equal
        } else if code < *start {
            crate::cmp::Ordering::Less
        } else {
            crate::cmp::Ordering::Greater
        }
    }) {
        // SAFETY: Index comes from statically generated table
        let code = *unsafe { plane.single_vals.get_unchecked(single_index) };
        let c = unsafe { char::from_u32_unchecked((plane_index * 0x10_000) | (code as u32)) };
        return [c, '\0', '\0'];
    }

    [c, '\0', '\0']
}

#[optimize(size)]
pub fn to_lower(c: char) -> [char; 3] {
    if c.is_ascii() {
        return [c.to_ascii_lowercase(), '\0', '\0'];
    }

    lookup(c, LOWERCASE_TABLES)
}

#[optimize(size)]
pub fn to_upper(c: char) -> [char; 3] {
    if c.is_ascii() {
        return [c.to_ascii_uppercase(), '\0', '\0'];
    }

    lookup(c, UPPERCASE_TABLES)
}
";
