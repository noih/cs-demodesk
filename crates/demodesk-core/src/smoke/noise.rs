//! Raw installed smoke noise volume; no procedural replacement or colour conversion.
use anyhow::{ensure, Context, Result};

pub const RESOURCE: &str = "materials/dev/noise/worley_perlin_z0000_tga_37c9ebc1.vtex";
pub const SIDE: usize = 128;

/// Borrow the source-qualified RGBA8 volume without decoding 128 image files.
pub fn rgba(resource: &[u8]) -> Result<&[u8]> {
    let integer = |at: usize| -> Result<usize> {
        Ok(u32::from_le_bytes(
            resource
                .get(at..at + 4)
                .context("truncated smoke noise resource")?
                .try_into()?,
        ) as usize)
    };
    let end = integer(0)?;
    ensure!(
        resource.get(4..8) == Some(&[12, 0, 1, 0]),
        "unsupported smoke noise resource version"
    );
    let table = 8usize
        .checked_add(integer(8)?)
        .context("smoke noise table overflow")?;
    let count = integer(12)?;
    ensure!(
        count <= 64 && table <= end && count * 12 <= end - table,
        "invalid smoke noise block table"
    );
    let mut data = None;
    for i in 0..count {
        let at = table + i * 12;
        let start = (at + 4)
            .checked_add(integer(at + 4)?)
            .context("smoke noise block overflow")?;
        let size = integer(at + 8)?;
        ensure!(
            start <= end && size <= end - start,
            "invalid smoke noise block bounds"
        );
        if resource.get(at..at + 4) == Some(b"DATA") {
            ensure!(data.is_none(), "duplicate smoke noise DATA");
            data = Some((start, size));
        }
    }
    let (start, size) = data.context("missing smoke noise DATA")?;
    let header = resource
        .get(start..start + size)
        .context("truncated smoke noise DATA")?;
    ensure!(
        size == 68 && start + size == end,
        "unsupported smoke noise DATA layout"
    );
    ensure!(
        header[..4] == [1, 0, 32, 0] && header[20..28] == [128, 0, 128, 0, 128, 0, 4, 1],
        "unsupported smoke noise texture format"
    );
    // One uncompressed mip: picmip, extra table, COMPRESSED_MIP_SIZE entry and payload.
    for (bytes, value) in header[28..]
        .chunks_exact(4)
        .zip([0u32, 8, 1, 4, 8, 12, 0, 8, 1, 8388608])
    {
        ensure!(
            bytes == value.to_le_bytes(),
            "unsupported smoke noise mip descriptor"
        );
    }
    let pixels = resource
        .get(end..)
        .context("truncated smoke noise pixels")?;
    ensure!(
        pixels.len() == SIDE * SIDE * SIDE * 4,
        "invalid smoke noise pixel length"
    );
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrows_one_raw_volume_and_rejects_other_encodings() {
        let mut bytes = vec![0; 96 + SIDE * SIDE * SIDE * 4];
        bytes[..16].copy_from_slice(&[96, 0, 0, 0, 12, 0, 1, 0, 8, 0, 0, 0, 1, 0, 0, 0]);
        bytes[16..28].copy_from_slice(&[b'D', b'A', b'T', b'A', 8, 0, 0, 0, 68, 0, 0, 0]);
        bytes[28..32].copy_from_slice(&[1, 0, 32, 0]);
        bytes[48..56].copy_from_slice(&[128, 0, 128, 0, 128, 0, 4, 1]);
        for (chunk, n) in bytes[56..96]
            .chunks_exact_mut(4)
            .zip([0u32, 8, 1, 4, 8, 12, 0, 8, 1, 8388608])
        {
            chunk.copy_from_slice(&n.to_le_bytes());
        }
        bytes[96] = 37;
        assert_eq!(rgba(&bytes).unwrap().as_ptr(), bytes[96..].as_ptr());
        assert_eq!(rgba(&bytes).unwrap()[0], 37);
        bytes[80] = 1; // compressed mip flag
        assert!(rgba(&bytes).is_err());
        bytes[80] = 0;
        assert!(rgba(&bytes[..bytes.len() - 1]).is_err());
    }
}
