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
    single_keys: Vec<HexEscape>,
    single_vals: Vec<HexEscape>,
    multi_keys: Vec<HexEscape>,
    multi_vals: Vec<[CharEscape; 3]>,
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
                plane.single_keys.push(HexEscape(key_code));
                plane.single_vals.push(HexEscape(val_code));
            }
            &chars => {
                plane.multi_keys.push(HexEscape(key_code));
                plane.multi_vals.push(chars.map(CharEscape));
            }
        }

        num_planes = num_planes.max(key_plane + 1);
    }
    planes.truncate(num_planes as usize);

    let mut tables = String::new();
    writeln!(tables, "static {case}CASE_TABLES: &[Plane; {}] = &[", num_planes)?;
    for plane in planes {
        writeln!(tables, "    Plane {{")?;
        for (name, table) in [
            ("single_keys", &plane.single_keys[..]),
            ("single_vals", &plane.single_vals[..]),
            ("multi_keys", &plane.multi_keys[..]),
        ] {
            writeln!(tables, "        // {} entries, {} bytes", table.len(), size_of_val(table))?;
            writeln!(tables, "        {name}: &[{}],", fmt_list(table))?;
        }
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

#[derive(Copy, Clone)]
struct CharEscape(char);

impl fmt::Debug for CharEscape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}'", self.0.escape_default())
    }
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
    single_keys: &'static [u16],
    single_vals: &'static [u16],
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

    if let Ok(single_index) = plane.single_keys.binary_search(&code) {
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
