//! The width and height of a PNG, JPEG, or WebP image, read from its
//! header. transom never decodes a picture, but it reads the size so it
//! can refuse one that is not square, or one small on the wire that would
//! decode to an enormous bitmap on a phone.

/// The width and height of an image of `content_type`, as its header
/// gives them, or `None` when the header is not one transom can read.
pub(crate) fn of(content_type: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    match content_type {
        "image/png" => png(bytes),
        "image/jpeg" => jpeg(bytes),
        "image/webp" => webp(bytes),
        _ => None,
    }
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?).into())
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A little-endian number of `width` bytes, at most four.
fn le(bytes: &[u8], at: usize, width: usize) -> Option<u32> {
    let mut number = [0; 4];
    number[..width].copy_from_slice(bytes.get(at..at + width)?);
    Some(u32::from_le_bytes(number))
}

/// The first chunk of a PNG is its header, `IHDR`, which starts with the
/// width and the height.
fn png(bytes: &[u8]) -> Option<(u32, u32)> {
    (bytes.get(12..16)? == b"IHDR").then_some(())?;
    Some((be32(bytes, 16)?, be32(bytes, 20)?))
}

/// A JPEG is a run of segments, each a marker and, for most, a length.
/// The first start-of-frame segment, any of `C0` to `CF` but the three
/// that are not frames, gives the height and then the width.
fn jpeg(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while bytes.get(at) == Some(&0xFF) {
        // A marker may follow any number of fill bytes.
        while bytes.get(at) == Some(&0xFF) {
            at += 1;
        }
        let marker = *bytes.get(at)?;
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            return Some((be16(bytes, at + 6)?, be16(bytes, at + 4)?));
        }
        at += 1 + usize::try_from(be16(bytes, at + 1)?).ok()?;
    }
    None
}

/// A WebP's first chunk says how its size is written: in a lossy frame's
/// header, in a lossless one's bit-packed header, or, for an extended
/// file, as the canvas size.
fn webp(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8 " => {
            (bytes.get(23..26)? == [0x9D, 0x01, 0x2A]).then_some(())?;
            Some((le(bytes, 26, 2)? & 0x3FFF, le(bytes, 28, 2)? & 0x3FFF))
        }
        b"VP8L" => {
            (*bytes.get(20)? == 0x2F).then_some(())?;
            let bits = le(bytes, 21, 4)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => Some((le(bytes, 24, 3)? + 1, le(bytes, 27, 3)? + 1)),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod testing {
    /// The start of a PNG of `width` by `height`: as much as transom reads.
    pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::of;
    use super::testing::png;

    /// A JPEG with an APP0 segment before its frame header, and `fill`
    /// bytes of padding before the frame header's marker.
    fn jpeg(width: u16, height: u16, fill: usize) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        bytes.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        bytes.extend(vec![0xFF; fill]);
        bytes.extend([0xFF, 0xC2, 0, 17, 8]);
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        bytes
    }

    fn webp(chunk: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        bytes.extend(chunk);
        bytes.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
        bytes.extend(data);
        bytes
    }

    fn lossy(width: u16, height: u16) -> Vec<u8> {
        let mut data = vec![0x10, 0x02, 0x00, 0x9D, 0x01, 0x2A];
        data.extend(width.to_le_bytes());
        data.extend(height.to_le_bytes());
        webp(*b"VP8 ", &data)
    }

    fn lossless(width: u32, height: u32) -> Vec<u8> {
        let bits = (width - 1) | ((height - 1) << 14);
        let mut data = vec![0x2F];
        data.extend(bits.to_le_bytes());
        webp(*b"VP8L", &data)
    }

    fn extended(width: u32, height: u32) -> Vec<u8> {
        let mut data = vec![0x10, 0, 0, 0];
        data.extend(&(width - 1).to_le_bytes()[..3]);
        data.extend(&(height - 1).to_le_bytes()[..3]);
        webp(*b"VP8X", &data)
    }

    #[test]
    fn reads_each_kind_of_header() {
        let cases = [
            ("image/png", png(512, 256), (512, 256)),
            ("image/jpeg", jpeg(300, 200, 0), (300, 200)),
            ("image/jpeg", jpeg(64, 64, 3), (64, 64)),
            ("image/webp", lossy(640, 480), (640, 480)),
            ("image/webp", lossless(1000, 2), (1000, 2)),
            ("image/webp", extended(4000, 4000), (4000, 4000)),
        ];
        for (content_type, bytes, size) in cases {
            assert_eq!(of(content_type, &bytes), Some(size), "{content_type}");
        }
    }

    #[test]
    fn refuses_a_header_it_cannot_read() {
        let mut no_frame = jpeg(10, 10, 0);
        no_frame.truncate(20);
        let mut bad_start = lossy(10, 10);
        bad_start[23] = 0;
        let mut bad_signature = lossless(10, 10);
        bad_signature[20] = 0;
        let mut not_ihdr = png(10, 10);
        not_ihdr[12] = b'X';
        let cases = [
            ("image/png", png(10, 10)[..20].to_vec()),
            ("image/png", not_ihdr),
            ("image/jpeg", no_frame),
            ("image/webp", bad_start),
            ("image/webp", bad_signature),
            ("image/webp", webp(*b"ALPH", &[0; 10])),
            ("image/gif", b"GIF89a\x01\0\x01\0".to_vec()),
        ];
        for (content_type, bytes) in cases {
            assert_eq!(of(content_type, &bytes), None, "{content_type} {bytes:?}");
        }
    }
}
