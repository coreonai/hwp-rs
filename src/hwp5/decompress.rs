use flate2::{Decompress, FlushDecompress, Status};

use crate::error::{HwpError, Result};

/// Decompress raw deflate data (wbits=-15, no zlib/gzip header).
///
/// HWP5 streams may have a trailing 8 bytes: `[crc32_le_4B][uncompressed_size_le_4B]`.
/// We detect and strip this trailer automatically.
pub fn decompress_stream(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    // First try decompressing the entire buffer as raw deflate.
    // If that consumes all bytes, there's no trailer.
    // If it leaves exactly 8 bytes, those are the CRC trailer.
    let mut decompressor = Decompress::new(false);
    let mut output = Vec::with_capacity(data.len() * 4);
    let mut buf = [0u8; 16384];

    loop {
        let in_before = decompressor.total_in() as usize;
        let out_before = decompressor.total_out() as usize;

        let input = &data[in_before..];
        let status = decompressor
            .decompress(input, &mut buf, FlushDecompress::Finish)
            .map_err(|e| HwpError::Decompress(format!("{e} (at input offset {in_before})")))?;

        let produced = decompressor.total_out() as usize - out_before;
        output.extend_from_slice(&buf[..produced]);

        match status {
            Status::StreamEnd => break,
            Status::Ok => continue,
            Status::BufError => {
                let consumed = decompressor.total_in() as usize - in_before;
                if consumed == 0 && produced == 0 {
                    break;
                }
            }
        }
    }

    let consumed = decompressor.total_in() as usize;
    let remaining = data.len() - consumed;

    // If exactly 8 bytes remain, verify CRC trailer
    if remaining == 8 {
        let trailer = &data[consumed..];
        let expected_crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
        let _expected_size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);

        let actual_crc = crc32_ieee(&output);
        if actual_crc != expected_crc {
            // CRC mismatch is non-fatal — some files have incorrect trailers
            eprintln!(
                "[hwp-rs] CRC mismatch: expected {expected_crc:#010x}, got {actual_crc:#010x} (non-fatal)"
            );
        }
    }

    Ok(output)
}

/// Decompress raw deflate data without any trailer handling.
pub fn decompress_raw(data: &[u8]) -> Result<Vec<u8>> {
    let mut decompressor = Decompress::new(false);
    let mut output = Vec::with_capacity(data.len() * 4);
    let mut buf = [0u8; 16384];

    loop {
        let in_before = decompressor.total_in() as usize;
        let out_before = decompressor.total_out() as usize;

        let input = &data[in_before..];
        let status = decompressor
            .decompress(input, &mut buf, FlushDecompress::None)
            .map_err(|e| HwpError::Decompress(e.to_string()))?;

        let produced = decompressor.total_out() as usize - out_before;
        output.extend_from_slice(&buf[..produced]);

        match status {
            Status::StreamEnd => break,
            Status::Ok => continue,
            Status::BufError => {
                let consumed = decompressor.total_in() as usize - in_before;
                if consumed == 0 && produced == 0 {
                    break;
                }
            }
        }
    }

    Ok(output)
}

/// IEEE CRC-32 (same as zlib crc32).
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(crc32_ieee(b""), 0x0000_0000);
    }

    #[test]
    fn test_crc32_known() {
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn test_decompress_raw_trivial() {
        use flate2::{Compress, FlushCompress};
        let input = b"Hello, HWP world!";
        let mut compressor = Compress::new(flate2::Compression::default(), false);
        let mut compressed = vec![0u8; 1024];
        compressor
            .compress(input, &mut compressed, FlushCompress::Finish)
            .unwrap();
        let compressed_len = compressor.total_out() as usize;
        compressed.truncate(compressed_len);

        let decompressed = decompress_raw(&compressed).unwrap();
        assert_eq!(&decompressed, input);
    }

    #[test]
    fn test_decompress_stream_with_trailer() {
        use flate2::{Compress, FlushCompress};
        let input = b"Test data for HWP decompression";
        let mut compressor = Compress::new(flate2::Compression::default(), false);
        let mut compressed = vec![0u8; 1024];
        compressor
            .compress(input, &mut compressed, FlushCompress::Finish)
            .unwrap();
        let compressed_len = compressor.total_out() as usize;
        compressed.truncate(compressed_len);

        // Append CRC trailer
        let crc = crc32_ieee(input);
        let size = input.len() as u32;
        compressed.extend_from_slice(&crc.to_le_bytes());
        compressed.extend_from_slice(&size.to_le_bytes());

        let decompressed = decompress_stream(&compressed).unwrap();
        assert_eq!(&decompressed[..], input);
    }

    #[test]
    fn test_decompress_stream_without_trailer() {
        use flate2::{Compress, FlushCompress};
        let input = b"No trailer here";
        let mut compressor = Compress::new(flate2::Compression::default(), false);
        let mut compressed = vec![0u8; 1024];
        compressor
            .compress(input, &mut compressed, FlushCompress::Finish)
            .unwrap();
        let compressed_len = compressor.total_out() as usize;
        compressed.truncate(compressed_len);

        // No trailer — should still work
        let decompressed = decompress_stream(&compressed).unwrap();
        assert_eq!(&decompressed[..], input);
    }
}
