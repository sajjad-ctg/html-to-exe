//! Convert a developer-provided `.png` into a fully packed
// multi-size Windows `.ico`, ready for `iconpatch`.
//!
//! Sizes follow desktop shipping convention: 256, 128, 64, 48, 32, 16.
//! The 256 entry is PNG-compressed (the Vista+ standard); smaller
//! entries are classic 32-bit BMP to maximize compatibility with
//! older shell surfaces (taskbar, menus, file dialogs).
//!
//! Non-square input is centered on a transparent square canvas first,
//! so icons never distort. Sizes larger than the source pixel data are
//! skipped (Windows picks the nearest available entry) except that the
//! smallest entry is always present so the group is never empty.

use image::{imageops, DynamicImage, GenericImageView, ImageFormat, RgbaImage};

const SIZES: [u32; 6] = [256, 128, 64, 48, 32, 16];

/// Decode `png`, pad to square, and pack all desktop sizes into one
/// `.ico` byte container. Errors carry short user-facing messages.
pub fn png_to_ico(png: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(png).map_err(|e| format!("cannot read png: {e}"))?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err("png has zero dimensions".into());
    }
    let square = pad_square(&img.to_rgba8());
    Ok(encode_ico(&square, &SIZES))
}

/// Center `img` on a transparent square canvas.
fn pad_square(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == h {
        return img.clone();
    }
    let side = w.max(h);
    let mut canvas = RgbaImage::new(side, side);
    let x = ((side - w) / 2) as i64;
    let y = ((side - h) / 2) as i64;
    imageops::overlay(&mut canvas, img, x, y);
    canvas
}

/// Build the ICO: directory entries first, image payloads after, in
/// the same order for both, with running offsets.
fn encode_ico(square: &RgbaImage, sizes: &[u32]) -> Vec<u8> {
    let (max_px, _) = square.dimensions();
    // keep everything the source can offer, plus the smallest size so
    // the group is never empty
    let sizes: Vec<u32> = sizes
        .iter()
        .copied()
        .filter(|&s| s <= max_px)
        .chain(std::iter::once(16))
        .collect::<Vec<_>>();
    let mut unique = Vec::new();
    for s in sizes {
        if !unique.contains(&s) {
            unique.push(s);
        }
    }

    let mut blobs: Vec<(u8, u8, u16, Vec<u8>)> = Vec::new(); // (w_byte, h_byte, bit_count, data)
    for &s in &unique {
        let scaled = imageops::resize(square, s, s, imageops::FilterType::CatmullRom);
        let data = if s == 256 {
            encode_png(&scaled)
        } else {
            encode_bmp(&scaled)
        };
        // 256 collapses to byte 0 in the directory entry
        blobs.push((
            if s == 256 { 0 } else { s as u8 },
            if s == 256 { 0 } else { s as u8 },
            32,
            data,
        ));
    }

    let count = blobs.len();
    let mut out = Vec::with_capacity(6 + count * 16 + blobs.iter().map(|b| b.3.len()).sum::<usize>());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(count as u16).to_le_bytes());

    let mut offset = (6 + count * 16) as u32;
    for &(w, h, bits, ref data) in &blobs {
        out.push(w);
        out.push(h);
        out.push(0); // palette count, unused
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&bits.to_le_bytes()); // bit count
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, _, _, data) in blobs {
        out.extend_from_slice(&data);
    }
    out
}

/// PNG-encoded entry (used for the 256px image).
fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut wrap = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut wrap, ImageFormat::Png)
        .expect("png encoding of an in-memory image cannot fail");
    wrap.into_inner()
}

/// classic BMP entry: BITMAPINFOHEADER + bottom-up BGRA pixels + an
/// all-zero AND mask (alpha lives in the pixel data).
fn encode_bmp(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let mask_row = ((w + 31) / 32) * 4;
    let mask_size = mask_row * h;

    let mut out = Vec::with_capacity(40 + (w * h * 4) as usize + mask_size as usize);
    // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&((h as i32) * 2).to_le_bytes()); // color + mask rows
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // compression: BI_RGB
    out.extend_from_slice(&((w * h * 4 + mask_size).to_le_bytes())); // image size
    out.extend_from_slice(&0u32.to_le_bytes()); // x pels per meter
    out.extend_from_slice(&0u32.to_le_bytes()); // y pels per meter
    out.extend_from_slice(&0u32.to_le_bytes()); // palette colors
    out.extend_from_slice(&0u32.to_le_bytes()); // important colors

    // pixels, bottom-up, BGRA
    for y in (0..h).rev() {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }

    // AND mask, all opaque
    out.extend(std::iter::repeat(0u8).take(mask_size as usize));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_png(size: u32) -> Vec<u8> {
        let mut img = RgbaImage::new(size, size);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = if (x + y) % 2 == 0 {
                image::Rgba([120, 150, 255, 255])
            } else {
                image::Rgba([20, 30, 80, 255])
            };
        }
        let mut wrap = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut wrap, ImageFormat::Png)
            .unwrap();
        wrap.into_inner()
    }

    fn read_u16(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
    }
    fn read_u32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn packs_all_desktop_sizes_from_a_256_png() {
        let ico = png_to_ico(&square_png(256)).unwrap();
        assert_eq!(read_u16(&ico, 2), 1, "type icon");
        assert_eq!(read_u16(&ico, 4), 6, "all six sizes present");
        // first entry is 256 -> 0/0 bytes, PNG payload
        assert_eq!(ico[6], 0);
        assert_eq!(ico[7], 0);
        let first_size = read_u32(&ico, 6 + 8);
        let first_off = read_u32(&ico, 6 + 12);
        assert_eq!(&ico[first_off as usize..first_off as usize + 4], b"\x89PNG");
        assert!(first_size > 4);
        // last entry: 16px, bmp payload, roughly header + pixels + mask
        let last = 6 + 5 * 16;
        assert_eq!(ico[last], 16);
        let last_size = read_u32(&ico, last + 8);
        let last_off = read_u32(&ico, last + 12);
        assert_eq!(
            (last_off + last_size) as usize,
            ico.len(),
            "last entry ends the file"
        );
        assert_eq!(last_size, 40 + 16 * 16 * 4 + ((16 + 31) / 32) * 4 * 16);
    }

    #[test]
    fn non_square_png_is_padded_not_distorted() {
        // 64x32 source; every produced entry must still be square
        let mut img = RgbaImage::new(64, 32);
        for (_, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([255, 0, 0, 255]);
        }
        let mut wrap = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut wrap, ImageFormat::Png)
            .unwrap();
        let ico = png_to_ico(&wrap.into_inner()).unwrap();
        let count = read_u16(&ico, 4) as usize;
        for i in 0..count {
            let base = 6 + i * 16;
            assert_eq!(ico[base], ico[base + 1], "square entries only");
        }
        assert_eq!(read_u16(&ico, 4), 4, "64/32/16 sourced + smallest kept");
    }

    #[test]
    fn tiny_png_still_produces_a_usable_group() {
        let ico = png_to_ico(&square_png(32)).unwrap();
        assert_eq!(read_u16(&ico, 4), 2, "32px source -> 32+16");
    }

    #[test]
    fn garbage_png_is_an_error_not_a_panic() {
        assert!(png_to_ico(b"definitely not png").is_err());
    }
}
