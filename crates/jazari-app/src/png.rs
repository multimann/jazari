//! A minimal PNG writer for window screenshots: 8-bit RGBA, no filtering,
//! zlib "stored" (uncompressed) deflate blocks. Files are larger than they
//! need to be, but the writer is small and needs no extra crate.

use std::fmt;

/// The 8-byte PNG signature.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// The largest payload of one stored deflate block.
const MAX_STORED: usize = 65_535;

/// Why an image cannot be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PngError {
    /// Width or height is zero or does not fit PNG's 31-bit fields.
    BadSize {
        /// Width in pixels.
        width: usize,
        /// Height in pixels.
        height: usize,
    },
    /// The pixel buffer is not `width × height × 4` bytes.
    BadLength {
        /// Bytes expected.
        expected: usize,
        /// Bytes given.
        actual: usize,
    },
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PngError::BadSize { width, height } => {
                f.write_str(&tr!("png.bad_size", width = width, height = height))
            }
            PngError::BadLength { expected, actual } => {
                f.write_str(&tr!("png.bad_length", expected = expected, actual = actual))
            }
        }
    }
}

impl std::error::Error for PngError {}

/// Encodes straight (unpremultiplied) RGBA pixels, row by row from the top.
pub fn encode_rgba(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, PngError> {
    let max = i32::MAX as usize;
    if width == 0 || height == 0 || width > max || height > max {
        return Err(PngError::BadSize { width, height });
    }
    let row = width
        .checked_mul(4)
        .ok_or(PngError::BadSize { width, height })?;
    let expected = row
        .checked_mul(height)
        .ok_or(PngError::BadSize { width, height })?;
    if rgba.len() != expected {
        return Err(PngError::BadLength {
            expected,
            actual: rgba.len(),
        });
    }

    // Scanlines, each prefixed with filter type 0 (None).
    let mut raw = Vec::with_capacity(expected + height);
    for line in rgba.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    // Bit depth 8, colour type 6 (RGBA), deflate, adaptive filtering, no
    // interlace.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut out = Vec::with_capacity(raw.len() + raw.len() / MAX_STORED * 5 + 64);
    out.extend_from_slice(&SIGNATURE);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

/// Appends a chunk: length, type, data, CRC of type and data.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// A zlib stream of stored deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let blocks = data.len().div_ceil(MAX_STORED).max(1);
    let mut out = Vec::with_capacity(data.len() + blocks * 5 + 6);
    // CMF: deflate, 32 KiB window; FLG: no dictionary, check bits so that
    // CMF·256 + FLG is a multiple of 31.
    out.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = data.chunks(MAX_STORED).peekable();
    if chunks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = chunks.next() {
        let last = chunks.peek().is_none();
        let len = block.len() as u16;
        out.push(u8::from(last));
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// The Adler-32 checksum of zlib.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 bytes is the most that can be summed before reducing.
    for block in data.chunks(5552) {
        for &byte in block {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// CRC-32 (IEEE, reflected), as PNG uses it.
struct Crc32 {
    value: u32,
}

impl Crc32 {
    fn new() -> Self {
        Self { value: !0 }
    }

    fn update(&mut self, data: &[u8]) {
        let table = crc_table();
        for &byte in data {
            let index = ((self.value ^ u32::from(byte)) & 0xFF) as usize;
            self.value = table[index] ^ (self.value >> 8);
        }
    }

    fn finish(&self) -> u32 {
        !self.value
    }
}

fn crc_table() -> &'static [u32; 256] {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *slot = c;
        }
        table
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crc(data: &[u8]) -> u32 {
        let mut c = Crc32::new();
        c.update(data);
        c.finish()
    }

    #[test]
    fn checksums_match_their_reference_values() {
        assert_eq!(crc(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        // Long input exercises the deferred modulo.
        let long = vec![0xFFu8; 100_000];
        let (mut a, mut b) = (1u64, 0u64);
        for &x in &long {
            a = (a + u64::from(x)) % 65_521;
            b = (b + a) % 65_521;
        }
        assert_eq!(adler32(&long), ((b << 16) | a) as u32);
    }

    #[test]
    fn one_pixel_image_layout() {
        let png = encode_rgba(1, 1, &[10, 20, 30, 255]).expect("encode");
        assert_eq!(&png[..8], &SIGNATURE);
        // IHDR right after the signature.
        assert_eq!(&png[8..16], &[0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        assert_eq!(&png[16..24], &[0, 0, 0, 1, 0, 0, 0, 1]);
        assert_eq!(&png[24..29], &[8, 6, 0, 0, 0]);
        // The file ends with the standard IEND chunk.
        assert_eq!(
            &png[png.len() - 12..],
            &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]
        );
        // The pixel bytes are stored verbatim after the filter byte.
        let idat = png
            .windows(4)
            .position(|w| w == b"IDAT")
            .expect("IDAT present");
        let body = &png[idat + 4..];
        assert_eq!(&body[..2], &[0x78, 0x01]);
        assert_eq!(&body[2..7], &[1, 5, 0, !5u8, 0xFF]);
        assert_eq!(&body[7..12], &[0, 10, 20, 30, 255]);
    }

    #[test]
    fn large_images_use_several_stored_blocks() {
        let (w, h) = (300, 300);
        let pixels = vec![7u8; w * h * 4];
        let png = encode_rgba(w, h, &pixels).expect("encode");
        let raw_len = h * (w * 4 + 1);
        let blocks = raw_len.div_ceil(MAX_STORED);
        assert!(blocks > 1);
        // Signature, three chunks of 12 bytes overhead, IHDR data, the zlib
        // header and checksum, and 5 bytes per block.
        assert_eq!(png.len(), 8 + 3 * 12 + 13 + 2 + 4 + blocks * 5 + raw_len);
    }

    #[test]
    fn bad_input_is_refused() {
        assert_eq!(
            encode_rgba(0, 4, &[]),
            Err(PngError::BadSize {
                width: 0,
                height: 4
            })
        );
        assert_eq!(
            encode_rgba(2, 2, &[0; 15]),
            Err(PngError::BadLength {
                expected: 16,
                actual: 15
            })
        );
        assert!(encode_rgba(usize::MAX, 2, &[]).is_err());
    }
}
