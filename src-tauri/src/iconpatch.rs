//! Windows icon patching for exported exes.
//!
//! Uses the Canon `BeginUpdateResource`/`UpdateResource`/`EndUpdateResource`
//! API — the same approach installers and PyInstaller use — to swap the
//! shell's neutral default icon for the user's game icon.
//!
//! Supported `.ico` files: the normal multi-image desktop format. Each
//! image inside becomes an `RT_ICON` entry; `RT_GROUP_ICON` id 1 is the
//! descriptor that references them, matching what `winresource` embeds
//! into the shell at build time (`shell/build.rs`).

use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
};
use windows::Win32::UI::WindowsAndMessaging::{RT_GROUP_ICON, RT_ICON};

/// The shell embeds exactly one icon group with resource id 1 and one
/// image (256px PNG). User images take ids 1..=n in the group.
const RESOURCE_ID_BASE: u32 = 1;
const GROUP_ICON_LANGID: u16 = 0x0409; // LANG_ENGLISH_US, rc.exe default

struct IcoImage {
    /// Raw payload bytes (PNG or BMP as stored in the .ico).
    data: Vec<u8>,
    /// Logical width, 256 collapses to 0 in the directory entry.
    width: u8,
    height: u8,
    color_count: u8,
    planes: u16,
    bit_count: u16,
}

/// Parse a `.ico` into its images. Manual parsing on purpose: the whole
/// format is a 6-byte directory plus 16-byte entries, and no crate
/// pulls its weight for that.
fn parse_ico(bytes: &[u8]) -> Result<Vec<IcoImage>, String> {
    if bytes.len() < 6 || u16::from_le_bytes(bytes[0..2].try_into().unwrap()) != 0 {
        return Err("not an .ico file".into());
    }
    let kind = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
    if kind != 1 {
        return Err("not an .ico file".into());
    }
    let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;
    if count == 0 || count > 64 || bytes.len() < 6 + count * 16 {
        return Err("malformed .ico directory".into());
    }

    let mut images = Vec::with_capacity(count);
    for i in 0..count {
        let base = 6 + i * 16;
        let read_u16 = |o: usize| u16::from_le_bytes(bytes[base + o..base + o + 2].try_into().unwrap());
        let read_u32 = |o: usize| u32::from_le_bytes(bytes[base + o..base + o + 4].try_into().unwrap());

        let width = bytes[base];
        let height = bytes[base + 1];
        let color_count = bytes[base + 2];
        let planes = read_u16(4);
        let bit_count = read_u16(6);
        let size = read_u32(8) as usize;
        let offset = read_u32(12) as usize;

        let data = bytes
            .get(offset..offset.checked_add(size).ok_or("malformed .ico size")?)
            .ok_or_else(|| format!("truncated .ico image #{i}"))?;
        if data.len() != size {
            return Err(format!("truncated .ico image #{i}"));
        }

        images.push(IcoImage {
            data: data.to_vec(),
            width,
            height,
            color_count,
            planes: planes.max(1),
            bit_count,
        });
    }
    Ok(images)
}

/// RT_GROUP_ICON payload: NEWHEADER + one NEWRESDIR per image.
fn group_blob(images: &[IcoImage]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(6 + images.len() * 14);
    blob.extend_from_slice(&0u16.to_le_bytes()); // reserved
    blob.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    blob.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (index, image) in images.iter().enumerate() {
        blob.push(image.width);
        blob.push(image.height);
        blob.push(image.color_count);
        blob.push(0); // reserved
        blob.extend_from_slice(&image.planes.to_le_bytes());
        blob.extend_from_slice(&image.bit_count.to_le_bytes());
        blob.extend_from_slice(&(image.data.len() as u32).to_le_bytes());
        blob.extend_from_slice(&((RESOURCE_ID_BASE + index as u32) as u16).to_le_bytes());
    }
    blob
}

/// Replace the exe's icon resources with the images from a `.ico`
/// file on disk. Convenience wrapper over `patch_icon_bytes`.
pub fn patch_icon(exe: &Path, ico: &Path) -> Result<(), String> {
    let bytes = std::fs::read(ico).map_err(|e| format!("cannot read icon: {e}"))?;
    patch_icon_bytes(exe, &bytes)
}

/// Resource id encoded as an MAKEINTRESOURCE-style pointer.
fn pc_resource(id: u32) -> PCWSTR {
    PCWSTR(id as usize as *const u16)
}

fn to_wide(text: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    text.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// Replace the exe's icon resources with the images inside `ico`
/// bytes. Accepts any well-formed multi-image desktop .ico container
/// (hand-written or produced by `png2ico`).
pub fn patch_icon_bytes(exe: &Path, ico: &[u8]) -> Result<(), String> {
    let images = parse_ico(ico)?;
    let group = group_blob(&images);

    let exe_wide = to_wide(exe);
    unsafe {
        let update = BeginUpdateResourceW(PCWSTR(exe_wide.as_ptr()), false)
            .map_err(|e| format!("BeginUpdateResource: {e}"))?;

        // Resource updates only become visible after a successful
        // EndUpdateResource(commit). Any failure in between discards.
        //
        // Order matters: id 1..=n are REPLACED (a delete-then-add on
        // the same id makes UpdateResourceW fail), and the housekeeping
        // sweep of stale images runs last, best-effort.
        let staged = (|| {
            // 1. add the user's images under consecutive ids (floating
            //    replaces whatever the shell shipped under each id)
            for (index, image) in images.iter().enumerate() {
                let id = RESOURCE_ID_BASE + index as u32;
                UpdateResourceW(
                    update,
                    RT_ICON,
                    pc_resource(id),
                    GROUP_ICON_LANGID,
                    Some(image.data.as_ptr().cast()),
                    image.data.len() as u32,
                )
                .map_err(|e| format!("cannot write icon image #{index}: {e}"))?;
            }

            // 2. point the group descriptor (id 1) at the new images
            UpdateResourceW(
                update,
                RT_GROUP_ICON,
                pc_resource(RESOURCE_ID_BASE),
                GROUP_ICON_LANGID,
                Some(group.as_ptr().cast()),
                group.len() as u32,
            )
            .map_err(|e| format!("cannot write icon group: {e}"))?;

            // No housekeeping deletions here: exports always patch a
            // fresh template copy (ids above the new count only exist
            // when re-patching an already-patched exe, which we never
            // do), and any deletion attempt — even of absent ids —
            // poisons the update handle and fails EndUpdateResource
            // with ERROR_INTERNAL_ERROR.
            Ok(())
        })();

        match staged {
            // commit
            Ok(()) => EndUpdateResourceW(update, false)
                .map_err(|e| format!("EndUpdateResource: {e}"))
                .map(|_| ()),
            // discard: exe on disk stays exactly as it was
            Err(message) => {
                let _ = EndUpdateResourceW(update, true);
                Err(message)
            }
        }
    }
}
