use once_cell::sync::Lazy;
use windows::Win32::System::LibraryLoader::GetModuleHandleA;

use super::memory::{at, bytes};

pub(super) static MSG_REPOSITORY: Lazy<Option<usize>> = Lazy::new(|| {
    let address = find_repository();
    if address.is_none() {
        log::warn!("Could not locate DS3 MsgRepository; item catalog will remain unavailable");
    }
    address
});

// DS3 TGA's MsgRepository signature, verified against the 1.15.2 executable.
const SIGNATURE: &[u8] =
    b"\x44\x8b\xf8\x44\x0f\xb6\xa7\0\0\0\0\x48\x8b\x1d\0\0\0\0\x48\x85\xdb\x75";

fn matches_signature(data: &[u8]) -> bool {
    data.iter()
        .zip(SIGNATURE)
        .enumerate()
        .all(|(index, (actual, expected))| matches!(index, 7..=10 | 14..=17) || actual == expected)
}

fn resolve_reference(code: &[u8], address: usize) -> Result<Option<usize>, ()> {
    let mut matches =
        code.windows(SIGNATURE.len()).enumerate().filter(|(_, window)| matches_signature(window));
    let Some((offset, instruction)) = matches.next() else { return Ok(None) };
    if matches.next().is_some() {
        return Err(());
    }
    let displacement: i32 = at(instruction, 14).ok_or(())?;
    Ok(Some(
        address
            .checked_add(offset)
            .and_then(|v| v.checked_add(18))
            .and_then(|v| v.checked_add_signed(displacement as isize))
            .ok_or(())?,
    ))
}

fn find_repository() -> Option<usize> {
    let base = unsafe { GetModuleHandleA(None).ok()?.0 as usize };
    let dos = bytes(base, 0x40)?;
    if dos.get(..2)? != b"MZ" {
        return None;
    }
    let pe_offset: u32 = at(&dos, 0x3c)?;
    if pe_offset > 1024 * 1024 {
        return None;
    }
    let pe = bytes(base.checked_add(pe_offset as usize)?, 0x108)?;
    let section_count: u16 = at(&pe, 6)?;
    let optional_size: u16 = at(&pe, 0x14)?;
    let image_size: u32 = at(&pe, 0x50)?;
    if pe.get(..4)? != b"PE\0\0"
        || at::<u16>(&pe, 0x18)? != 0x20b
        || section_count == 0
        || section_count > 96
        || optional_size < 0x70
    {
        return None;
    }
    let section_address =
        base.checked_add(pe_offset as usize)?.checked_add(0x18 + usize::from(optional_size))?;
    let sections = bytes(section_address, usize::from(section_count) * 0x28)?;
    let mut result = None;
    for section in sections.as_chunks::<0x28>().0 {
        if section.get(..8)? != b".text\0\0\0" {
            continue;
        }
        let size: u32 = at(section, 8)?;
        let offset: u32 = at(section, 0xc)?;
        if offset.checked_add(size)? > image_size {
            return None;
        }
        let address = base.checked_add(offset as usize)?;
        let code = bytes(address, size as usize)?;
        if let Some(repository) = resolve_reference(&code, address).ok()? {
            if repository < base
                || repository.checked_add(8)? > base.checked_add(image_size as usize)?
                || result.replace(repository).is_some()
            {
                return None;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_reference_uses_signed_rip_displacement_and_rejects_ambiguity() {
        let mut code = SIGNATURE.to_vec();
        code[14..18].copy_from_slice(&(-0x30i32).to_le_bytes());
        assert_eq!(resolve_reference(&code, 0x1000), Ok(Some(0xfe2)));
        assert_eq!(resolve_reference(&code[..code.len() - 1], 0x1000), Ok(None));
        code.extend_from_within(..);
        assert_eq!(resolve_reference(&code, 0x1000), Err(()));
        code[0] = 0;
        assert_eq!(resolve_reference(&code, 0x1000), Ok(Some(0xff8)));
    }
}
