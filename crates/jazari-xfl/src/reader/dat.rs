//! Bitmap media: Flash's `bin/*.dat` format and raw image files.
//!
//! The `.dat` layout follows JPEXS (`ImageBinDataGenerator.java` writes it,
//! `LosslessImageBinDataReader.java` reads it), with the corrections in
//! `docs/research/xfl-format.md` (F24-F26 and the verifier's notes):
//!
//! ```text
//! lossless:  u8 major (0, 2 or 3) | u8 minor (= 5) | u16 rowBytes | u16 width
//!            | u16 height | 4 × u32 frame bounds in twips
//!            | major 3: u8 flags (bit 0 = has alpha), u8 variant
//!            | major 2: u8 variant            major 0: nothing (raw, opaque)
//!            | variant 1: [u16 len][len bytes]… u16 0 = one zlib stream
//!            | variant 0: the raw plane
//!            plane = height rows of rowBytes, pixels A,R,G,B premultiplied
//! palette:   03 03 | same header to byte 24 | u8 has alpha | u8 colour count
//!            (0 = 256) | 2 unknown bytes | count × R,G,B,A (straight)
//!            | chunked zlib indices, rows padded to rowBytes
//! JPEG:      the JPEG stream (FF D8 FF …) followed by 4 × u32 bounds
//! ```
//!
//! All numbers are little-endian.
//!
//! **The lossless plane is kept premultiplied, exactly as stored.** It is the
//! plane Flash publishes: in the `bitmaps` fixture the `.dat` plane is
//! byte-identical to the DefineBitsLossless2 data of the SWF that Flash 8
//! published. Relative to the imported image (straight `c`, `a`), Flash
//! stores semi-transparent pixels (`0 < a < 255`) with colours
//! `floor(c × a / 256)` and alpha `a + 1` (checked pixel by pixel against the
//! fixture's source PNG; it is the rule of JPEXS's writer, research F24
//! correction), and shifts some opaque channels by 1 on import. So a
//! semi-transparent stored alpha is one step too high, and Flash Player shows
//! it that way too. JPEXS's reader undoes it (`a − 1`, then `c × 256 / a`) to
//! recover the imported file; doing that here would lose information and
//! move the published pixels away from Flash's. Palette bitmaps and PNG files
//! are straight RGBA; JPEG is passed through.

use std::io::Read;

use flate2::read::{DeflateDecoder, ZlibDecoder};

/// Largest decoded plane accepted (64 Mi pixels); larger headers are treated
/// as damaged rather than allocated.
const MAX_PLANE_BYTES: usize = 256 << 20;

/// Size of the all-zero preset dictionary some `.dat` streams need.
const ZERO_DICTIONARY: usize = 32 * 1024;

/// A decoded bitmap.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decoded {
    /// RGBA pixels, `width × height × 4` bytes, row-major.
    Rgba {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Whether colours are premultiplied by alpha.
        premultiplied: bool,
        /// The pixels.
        pixels: Vec<u8>,
    },
    /// JPEG data from SOI to EOI, with the size read from its SOF marker.
    Jpeg {
        /// Width in pixels, if a frame header was found.
        width: Option<u32>,
        /// Height in pixels, if a frame header was found.
        height: Option<u32>,
        /// The JPEG stream.
        bytes: Vec<u8>,
    },
}

/// Decodes bitmap bytes from `bin/` or `LIBRARY/`, sniffing the format.
pub(crate) fn decode_media(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let jpeg = strip_jpeg_trailer(bytes);
        let size = jpeg_size(jpeg);
        return Ok(Decoded::Jpeg {
            width: size.map(|s| s.0),
            height: size.map(|s| s.1),
            bytes: jpeg.to_vec(),
        });
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return decode_png(bytes);
    }
    if bytes.starts_with(b"GIF8") {
        return Err("GIF bitmaps are not supported yet".to_owned());
    }
    match bytes {
        [0 | 2 | 3, 5, ..] => decode_lossless(bytes),
        [3, 3, ..] => decode_palette(bytes),
        [a, b, ..] => Err(format!(
            "unknown bitmap format (first bytes {a:02X} {b:02X})"
        )),
        _ => Err("the bitmap data is empty".to_owned()),
    }
}

/// Little-endian `u16` at `offset`.
fn u16_at(b: &[u8], offset: usize) -> Option<usize> {
    let pair = b.get(offset..offset + 2)?;
    Some(usize::from(u16::from_le_bytes([pair[0], pair[1]])))
}

/// The common 24-byte header: row bytes, width, height.
fn header(b: &[u8]) -> Result<(usize, usize, usize), String> {
    match (u16_at(b, 2), u16_at(b, 4), u16_at(b, 6)) {
        (Some(row), Some(w), Some(h)) if b.len() >= 24 => Ok((row, w, h)),
        _ => Err("the bitmap header is truncated".to_owned()),
    }
}

/// Decodes the 32-bit lossless format (major 0, 2 or 3; minor 5).
fn decode_lossless(b: &[u8]) -> Result<Decoded, String> {
    let (row, w, h) = header(b)?;
    let byte = |i: usize| {
        b.get(i)
            .copied()
            .ok_or_else(|| "the bitmap header is truncated".to_owned())
    };
    // `None`: the format version does not say; decided from the data below.
    let (has_alpha, variant, offset) = match b[0] {
        0 => (Some(false), 0, 24),
        2 => (None, byte(24)?, 25),
        _ => (Some(byte(24)? & 1 != 0), byte(25)?, 26),
    };
    let stride = row.max(w * 4);
    let need = stride * h;
    if need > MAX_PLANE_BYTES {
        return Err(format!("the bitmap is too large ({w} × {h})"));
    }
    let plane = match variant {
        0 => b
            .get(offset..offset + need)
            .ok_or_else(|| format!("the raw pixel plane is short ({w} × {h})"))?
            .to_vec(),
        1 => inflate_chunks(b.get(offset..).unwrap_or_default(), need)?,
        v => return Err(format!("unknown lossless bitmap variant {v}")),
    };
    let has_alpha = has_alpha.unwrap_or_else(|| {
        // Version 2 has no flags byte: an all-zero alpha plane means opaque.
        (0..w * h).any(|i| plane[(i / w) * stride + (i % w) * 4] != 0)
    });
    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let s = y * stride + x * 4;
            let (a, r, g, bl) = (plane[s], plane[s + 1], plane[s + 2], plane[s + 3]);
            let px = if has_alpha {
                [r, g, bl, a]
            } else {
                [r, g, bl, 255]
            };
            pixels.extend_from_slice(&px);
        }
    }
    Ok(rgba(w, h, true, pixels))
}

/// Decodes the 8-bit palette format (`03 03`).
fn decode_palette(b: &[u8]) -> Result<Decoded, String> {
    let (row, w, h) = header(b)?;
    let has_alpha = b.get(24).is_some_and(|&v| v != 0);
    let count = match b.get(25) {
        Some(0) => 256,
        Some(&n) => usize::from(n),
        None => return Err("the bitmap header is truncated".to_owned()),
    };
    let palette = b
        .get(28..28 + count * 4)
        .ok_or_else(|| "the palette is truncated".to_owned())?;
    let stride = row.max(w);
    let need = stride * h;
    if need > MAX_PLANE_BYTES {
        return Err(format!("the bitmap is too large ({w} × {h})"));
    }
    let indices = inflate_chunks(b.get(28 + count * 4..).unwrap_or_default(), need)?;
    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let i = usize::from(indices[y * stride + x]);
            let px = match palette.get(i * 4..i * 4 + 4) {
                Some(c) => [c[0], c[1], c[2], if has_alpha { c[3] } else { 255 }],
                None => [0, 0, 0, 0],
            };
            pixels.extend_from_slice(&px);
        }
    }
    Ok(rgba(w, h, false, pixels))
}

/// Builds a [`Decoded::Rgba`]; the dimensions come from `u16` fields.
fn rgba(w: usize, h: usize, premultiplied: bool, pixels: Vec<u8>) -> Decoded {
    Decoded::Rgba {
        width: u32::try_from(w).unwrap_or(u32::MAX),
        height: u32::try_from(h).unwrap_or(u32::MAX),
        premultiplied,
        pixels,
    }
}

/// Joins `[u16 len][len bytes]` records up to a zero length and inflates the
/// result to at least `need` bytes.
fn inflate_chunks(data: &[u8], need: usize) -> Result<Vec<u8>, String> {
    let mut stream = Vec::with_capacity(data.len());
    let mut rest = data;
    while let Some(len) = u16_at(rest, 0) {
        rest = &rest[2..];
        if len == 0 {
            break;
        }
        let len = len.min(rest.len());
        stream.extend_from_slice(&rest[..len]);
        rest = &rest[len..];
    }
    inflate(&stream, need)
}

/// Inflates a zlib stream to `need` bytes. Bytes past `need` are ignored, and
/// so is a bad checksum once `need` bytes are out. Streams that ask for a
/// preset dictionary get 32 KiB of zeros.
pub(crate) fn inflate(stream: &[u8], need: usize) -> Result<Vec<u8>, String> {
    let wants_dictionary = stream.len() >= 2 && stream[0] & 0x0F == 8 && stream[1] & 0x20 != 0;
    if wants_dictionary {
        // 2-byte header, then the 4-byte dictionary id.
        return inflate_raw_with_zero_dictionary(stream.get(6..).unwrap_or_default(), need);
    }
    let mut out = Vec::with_capacity(need.min(1 << 24));
    let result = ZlibDecoder::new(stream)
        .take(need as u64)
        .read_to_end(&mut out);
    if out.len() >= need {
        return Ok(out);
    }
    // Some files only inflate with the zero dictionary even without the
    // header flag; try that before giving up.
    if let Ok(retry) = inflate_raw_with_zero_dictionary(stream.get(2..).unwrap_or_default(), need) {
        return Ok(retry);
    }
    match result {
        Err(e) => Err(format!("the bitmap data does not inflate: {e}")),
        Ok(_) => Err(format!(
            "the bitmap data is short: {} of {need} bytes",
            out.len()
        )),
    }
}

/// Inflates raw deflate data as if 32 KiB of zeros preceded it.
///
/// A stored block holding the zeros is put in front of the data. Stored
/// blocks end on a byte boundary, so the original blocks follow unchanged and
/// their back-references can reach into the zeros. This works with any
/// deflate backend, unlike `set_dictionary`.
fn inflate_raw_with_zero_dictionary(raw: &[u8], need: usize) -> Result<Vec<u8>, String> {
    // BFINAL = 0, BTYPE = 00 (stored), LEN = 0x8000, NLEN = 0x7FFF.
    let mut prefix = vec![0x00, 0x00, 0x80, 0xFF, 0x7F];
    prefix.resize(prefix.len() + ZERO_DICTIONARY, 0);
    let mut out = Vec::with_capacity((need + ZERO_DICTIONARY).min(1 << 24));
    let result = DeflateDecoder::new(prefix.as_slice().chain(raw))
        .take((need + ZERO_DICTIONARY) as u64)
        .read_to_end(&mut out);
    if out.len() >= need + ZERO_DICTIONARY {
        out.drain(..ZERO_DICTIONARY);
        return Ok(out);
    }
    match result {
        Err(e) => Err(format!("the bitmap data does not inflate: {e}")),
        Ok(_) => Err("the bitmap data is short".to_owned()),
    }
}

/// The JPEG stream without the 16-byte bounds trailer Flash appends.
fn strip_jpeg_trailer(b: &[u8]) -> &[u8] {
    const EOI: [u8; 2] = [0xFF, 0xD9];
    if b.ends_with(&EOI) {
        return b;
    }
    if b.len() >= 18 && b[b.len() - 18..b.len() - 16] == EOI {
        return &b[..b.len() - 16];
    }
    match b.windows(2).rposition(|w| w == EOI) {
        Some(i) => &b[..i + 2],
        None => b,
    }
}

/// Width and height from the first SOF marker of a JPEG stream.
pub(crate) fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2; // after SOI
    while i + 1 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        i += 2;
        match marker {
            // Fill bytes, standalone markers.
            0xFF => i -= 1,
            0x00 | 0x01 | 0xD0..=0xD8 => {}
            0xD9 | 0xDA => return None,
            _ => {
                let len = usize::from(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]));
                let is_sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
                if is_sof {
                    let h = u16::from_be_bytes([*b.get(i + 3)?, *b.get(i + 4)?]);
                    let w = u16::from_be_bytes([*b.get(i + 5)?, *b.get(i + 6)?]);
                    return Some((u32::from(w), u32::from(h)));
                }
                i += len;
            }
        }
    }
    None
}

/// Decodes a PNG file to straight RGBA.
fn decode_png(bytes: &[u8]) -> Result<Decoded, String> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| format!("the PNG cannot be decoded: {e}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Ok(Decoded::Rgba {
        width,
        height,
        premultiplied: false,
        pixels: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    /// Premultiplied A,R,G,B plane of straight RGBA pixels, rounded as in
    /// the fixture: colour × a / 255, alpha unchanged.
    fn argb_plane(pixels: &[[u8; 4]]) -> Vec<u8> {
        let mut plane = Vec::new();
        for &[r, g, b, a] in pixels {
            let pre = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
            plane.extend_from_slice(&[a, pre(r), pre(g), pre(b)]);
        }
        plane
    }

    /// The RGBA pixels the decoder must return for a plane: the same bytes,
    /// reordered.
    fn rgba_of_plane(plane: &[u8]) -> Vec<u8> {
        plane
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[1], p[2], p[3], p[0]])
            .collect()
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    /// Frames a stream as `[u16 len][bytes]… u16 0` records of `chunk` bytes.
    fn chunked(stream: &[u8], chunk: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for part in stream.chunks(chunk) {
            out.extend_from_slice(&(part.len() as u16).to_le_bytes());
            out.extend_from_slice(part);
        }
        out.extend_from_slice(&[0, 0]);
        out
    }

    fn header_bytes(major: u8, minor: u8, row: u16, w: u16, h: u16) -> Vec<u8> {
        let mut b = vec![major, minor];
        for v in [row, w, h] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for v in [0u32, u32::from(w) * 20, 0, u32::from(h) * 20] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    fn rgba_of(d: Decoded) -> (u32, u32, bool, Vec<u8>) {
        match d {
            Decoded::Rgba {
                width,
                height,
                premultiplied,
                pixels,
            } => (width, height, premultiplied, pixels),
            Decoded::Jpeg { .. } => panic!("expected pixels"),
        }
    }

    const PIXELS: [[u8; 4]; 6] = [
        [255, 0, 0, 255],
        [255, 255, 255, 128],
        [0, 0, 255, 0],
        [200, 100, 50, 1],
        [10, 20, 30, 254],
        [255, 255, 255, 64],
    ];

    #[test]
    fn major_3_chunked_zlib_with_alpha() {
        let plane = argb_plane(&PIXELS);
        let mut dat = header_bytes(3, 5, 12, 3, 2);
        dat.extend_from_slice(&[1, 1]); // has alpha, chunked zlib
        dat.extend_from_slice(&chunked(&zlib(&plane), 7));
        dat.extend_from_slice(&[0xAA; 5]); // trailing junk is ignored
        let (w, h, pre, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!((w, h, pre), (3, 2, true));
        assert_eq!(px, rgba_of_plane(&plane));
        // The stored premultiplied values are kept: white at alpha 128.
        assert_eq!(&px[4..8], &[128, 128, 128, 128]);
    }

    #[test]
    fn major_3_raw_without_alpha_is_opaque() {
        let mut dat = header_bytes(3, 5, 8, 2, 1);
        dat.extend_from_slice(&[0, 0]); // no alpha, raw
        dat.extend_from_slice(&[0, 10, 20, 30, 7, 40, 50, 60]);
        let (_, _, _, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!(px, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn row_padding_is_skipped() {
        // rowBytes 12 for a 2-pixel row: 4 bytes of padding per row.
        let mut dat = header_bytes(3, 5, 12, 2, 2);
        dat.extend_from_slice(&[0, 0]);
        dat.extend_from_slice(&[0, 1, 2, 3, 0, 4, 5, 6, 9, 9, 9, 9]);
        dat.extend_from_slice(&[0, 7, 8, 9, 0, 10, 11, 12, 9, 9, 9, 9]);
        let (_, _, _, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!(
            px,
            vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
    }

    #[test]
    fn major_2_has_a_variant_byte_only() {
        let plane = argb_plane(&PIXELS[..2]);
        let mut dat = header_bytes(2, 5, 8, 2, 1);
        dat.push(1); // variant: chunked zlib
        dat.extend_from_slice(&chunked(&zlib(&plane), 2048));
        let (_, _, _, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!(px, rgba_of_plane(&plane));

        // An all-zero alpha plane in version 2 means an opaque bitmap.
        let mut dat = header_bytes(2, 5, 4, 1, 1);
        dat.push(0);
        dat.extend_from_slice(&[0, 9, 8, 7]);
        let (_, _, _, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!(px, vec![9, 8, 7, 255]);
    }

    #[test]
    fn major_0_is_raw_and_opaque() {
        let mut dat = header_bytes(0, 5, 4, 1, 1);
        dat.extend_from_slice(&[0, 1, 2, 3]);
        let (_, _, _, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!(px, vec![1, 2, 3, 255]);
    }

    #[test]
    fn preset_zero_dictionary_streams_inflate() {
        use flate2::{Compress, FlushCompress};
        let plane: Vec<u8> = [vec![0u8; 64], argb_plane(&PIXELS)].concat();
        let mut c = Compress::new(Compression::fast(), true);
        c.set_dictionary(&vec![0u8; ZERO_DICTIONARY]).unwrap();
        let mut stream = Vec::with_capacity(plane.len() * 2 + 64);
        c.compress_vec(&plane, &mut stream, FlushCompress::Finish)
            .unwrap();
        assert_ne!(stream[1] & 0x20, 0, "the stream asks for a dictionary");

        // 16 transparent pixels, then PIXELS: 22 pixels in a 22 × 1 bitmap.
        let mut dat = header_bytes(3, 5, 88, 22, 1);
        dat.extend_from_slice(&[1, 1]);
        dat.extend_from_slice(&chunked(&stream, 2048));
        let (_, _, _, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!(px, rgba_of_plane(&plane));
    }

    #[test]
    fn eight_bit_palette() {
        let mut dat = header_bytes(3, 3, 4, 3, 2); // rows padded to 4 bytes
        dat.extend_from_slice(&[1, 2, 0, 0]); // alpha, 2 colours, prefix
        dat.extend_from_slice(&[255, 0, 0, 255, 0, 0, 255, 128]);
        let indices = [0u8, 1, 0, 9, 1, 1, 5, 9]; // index 5 is out of range
        dat.extend_from_slice(&chunked(&zlib(&indices), 2048));
        let (w, h, pre, px) = rgba_of(decode_media(&dat).unwrap());
        assert_eq!((w, h, pre), (3, 2, false));
        assert_eq!(
            px,
            vec![
                255, 0, 0, 255, 0, 0, 255, 128, 255, 0, 0, 255, //
                0, 0, 255, 128, 0, 0, 255, 128, 0, 0, 0, 0,
            ]
        );
    }

    /// A minimal JPEG-shaped stream: SOI, APP0, SOF0 (w × h), SOS, EOI.
    fn fake_jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46];
        j.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
        j.extend_from_slice(&h.to_be_bytes());
        j.extend_from_slice(&w.to_be_bytes());
        j.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        j.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x12, 0x34, 0xFF, 0xD9]);
        j
    }

    #[test]
    fn jpeg_trailer_is_stripped_and_size_read() {
        let jpeg = fake_jpeg(160, 120);
        let mut dat = jpeg.clone();
        for v in [0u32, 3200, 0, 2400] {
            dat.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(
            decode_media(&dat).unwrap(),
            Decoded::Jpeg {
                width: Some(160),
                height: Some(120),
                bytes: jpeg.clone()
            }
        );
        // Without a trailer the bytes pass through unchanged.
        assert_eq!(
            decode_media(&jpeg).unwrap(),
            Decoded::Jpeg {
                width: Some(160),
                height: Some(120),
                bytes: jpeg
            }
        );
    }

    #[test]
    fn png_files_decode_to_straight_rgba() {
        let mut img = image::RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([1, 2, 3, 4]));
        img.put_pixel(1, 0, image::Rgba([5, 6, 7, 255]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (w, h, pre, px) = rgba_of(decode_media(&png).unwrap());
        assert_eq!((w, h, pre), (2, 1, false));
        assert_eq!(px, vec![1, 2, 3, 4, 5, 6, 7, 255]);
    }

    #[test]
    fn damaged_or_unknown_data_is_an_error() {
        assert!(decode_media(b"GIF89a").is_err());
        assert!(decode_media(&[]).is_err());
        assert!(decode_media(&[7, 7, 7]).is_err());
        assert!(decode_media(&[3, 5, 0]).is_err(), "truncated header");
        let mut dat = header_bytes(3, 5, 400, 100, 100);
        dat.extend_from_slice(&[1, 1]);
        dat.extend_from_slice(&chunked(&zlib(&[0; 16]), 2048));
        assert!(decode_media(&dat).is_err(), "short plane");
        let mut dat = header_bytes(3, 5, 4, 1, 1);
        dat.extend_from_slice(&[1, 7]);
        assert!(decode_media(&dat).is_err(), "unknown variant");
    }
}
