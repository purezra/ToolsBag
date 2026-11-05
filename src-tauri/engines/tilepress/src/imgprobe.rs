//! 只读文件头解析图片尺寸，不解码像素。
//!
//! 采用**分块渐进读**：先读 4KB，绝大多数图片在这一步就能拿到尺寸；
//! 带 EXIF 缩略图的 JPEG 的 SOF 可能靠后，再依次补读到 64KB / 256KB / 512KB。
//! 平均每张只读 4KB，2000 张的探测可以做到 1 秒内完成。

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// 渐进读取的阶梯（字节）
const STEPS: [usize; 4] = [4 * 1024, 64 * 1024, 256 * 1024, 512 * 1024];

pub fn probe(path: &Path) -> Option<(u32, u32)> {
    let mut f = File::open(path).ok()?;
    let mut buf = Vec::with_capacity(STEPS[STEPS.len() - 1]);

    for &step in STEPS.iter() {
        if buf.len() < step {
            let need = step - buf.len();
            let mut chunk = vec![0u8; need];
            let mut got = 0usize;
            while got < need {
                match f.read(&mut chunk[got..]) {
                    Ok(0) => break,
                    Ok(k) => got += k,
                    Err(_) => break,
                }
            }
            buf.extend_from_slice(&chunk[..got]);
        }
        if let Some(d) = detect(&buf) {
            return Some(d);
        }
        // 已经读到文件末尾，再补读也没有意义
        if buf.len() < step {
            break;
        }
    }
    None
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn le16(b: &[u8]) -> u32 {
    u16::from_le_bytes([b[0], b[1]]) as u32
}

pub fn detect(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 26 {
        return None;
    }
    // PNG
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        let w = be32(&b[16..20]);
        let h = be32(&b[20..24]);
        if w > 0 && h > 0 {
            return Some((w, h));
        }
        return None;
    }
    // JPEG
    if b[0] == 0xFF && b[1] == 0xD8 {
        return jpeg(b);
    }
    // GIF
    if b.starts_with(b"GIF8") {
        return Some((le16(&b[6..8]), le16(&b[8..10])));
    }
    // BMP
    if b.starts_with(b"BM") && b.len() > 26 {
        let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]).unsigned_abs();
        let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]).unsigned_abs();
        if w > 0 && h > 0 {
            return Some((w, h));
        }
    }
    // AVIF / HEIF：找 ispe box（宽高，大端）
    if b.len() >= 12 && &b[4..8] == b"ftyp" && matches!(&b[8..12], b"avif" | b"avis" | b"mif1") {
        if let Some(d) = ispe(b) {
            return Some(d);
        }
    }
    // WebP
    if b.len() > 30 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        match &b[12..16] {
            b"VP8X" => {
                let w = 1 + (b[24] as u32 | (b[25] as u32) << 8 | (b[26] as u32) << 16);
                let h = 1 + (b[27] as u32 | (b[28] as u32) << 8 | (b[29] as u32) << 16);
                return Some((w, h));
            }
            b"VP8 " => {
                let mut i = 20;
                while i + 10 < b.len() {
                    if b[i] == 0x9D && b[i + 1] == 0x01 && b[i + 2] == 0x2A {
                        let w = (le16(&b[i + 3..i + 5]) & 0x3FFF) as u32;
                        let h = (le16(&b[i + 5..i + 7]) & 0x3FFF) as u32;
                        if w > 0 && h > 0 {
                            return Some((w, h));
                        }
                        return None;
                    }
                    i += 1;
                }
            }
            b"VP8L" => {
                if b.len() > 25 && b[20] == 0x2F {
                    let bits = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
                    let w = (bits & 0x3FFF) + 1;
                    let h = ((bits >> 14) & 0x3FFF) + 1;
                    return Some((w, h));
                }
            }
            _ => {}
        }
    }
    // TIFF (LE / BE)
    if b.starts_with(b"II\x2A\x00") || b.starts_with(b"MM\x00\x2A") {
        return tiff(b);
    }
    None
}

fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
            i += 2;
            continue;
        }
        let len = ((b[i + 2] as usize) << 8) | b[i + 3] as usize;
        let is_sof = matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
        if is_sof {
            let h = ((b[i + 5] as u32) << 8) | b[i + 6] as u32;
            let w = ((b[i + 7] as u32) << 8) | b[i + 8] as u32;
            if w > 0 && h > 0 {
                return Some((w, h));
            }
            return None;
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn tiff(b: &[u8]) -> Option<(u32, u32)> {
    let le = b.starts_with(b"II");
    let rd16 = |o: usize| -> u32 {
        if le {
            u16::from_le_bytes([b[o], b[o + 1]]) as u32
        } else {
            u16::from_be_bytes([b[o], b[o + 1]]) as u32
        }
    };
    let rd32 = |o: usize| -> u32 {
        if le {
            u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        } else {
            u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        }
    };
    let ifd = rd32(4) as usize;
    if ifd + 2 > b.len() {
        return None;
    }
    let cnt = rd16(ifd) as usize;
    let (mut w, mut h) = (0u32, 0u32);
    for k in 0..cnt {
        let e = ifd + 2 + k * 12;
        if e + 12 > b.len() {
            break;
        }
        let tag = rd16(e);
        let val = rd32(e + 8);
        match tag {
            256 => w = val,
            257 => h = val,
            _ => {}
        }
    }
    if w > 0 && h > 0 {
        Some((w, h))
    } else {
        None
    }
}

/// 在 ISOBMFF 里找 ispe box：["ispe"(4)][ver+flags(4)][width(4)][height(4)]
fn ispe(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 0usize;
    while i + 16 <= b.len() {
        if &b[i..i + 4] == b"ispe" {
            let w = be32(&b[i + 8..i + 12]);
            let h = be32(&b[i + 12..i + 16]);
            if w > 0 && h > 0 {
                return Some((w, h));
            }
        }
        i += 1;
    }
    None
}
