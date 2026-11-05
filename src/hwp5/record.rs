use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read};

use crate::error::{HwpError, Result};

/// A parsed record header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    /// Tag ID (10 bits, 0–1023).
    pub tag: u16,
    /// Nesting level (10 bits).
    pub level: u16,
    /// Payload size in bytes.
    pub size: u32,
}

impl RecordHeader {
    /// Decode a record header from a packed u32.
    /// Layout: tag = bits[0:9], level = bits[10:19], size = bits[20:31]
    /// If size == 0xFFF, the next u32 is the actual size (extended).
    pub fn from_u32(header: u32) -> Self {
        let tag = (header & 0x3FF) as u16;
        let level = ((header >> 10) & 0x3FF) as u16;
        let size = (header >> 20) & 0xFFF;
        Self { tag, level, size }
    }
}

/// A record: header + payload bytes.
#[derive(Debug, Clone)]
pub struct Record {
    pub header: RecordHeader,
    pub payload: Vec<u8>,
}

/// Iterator over records in a byte stream.
pub struct RecordIter<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> RecordIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }
}

impl<'a> Iterator for RecordIter<'a> {
    type Item = Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        let raw_header = match self.cursor.read_u32::<LittleEndian>() {
            Ok(v) => v,
            Err(_) => return None, // EOF
        };

        let mut rh = RecordHeader::from_u32(raw_header);

        // Extended size: if size field is 0xFFF, read next u32 as actual size
        if rh.size == 0xFFF {
            match self.cursor.read_u32::<LittleEndian>() {
                Ok(ext) => rh.size = ext,
                Err(e) => return Some(Err(HwpError::Io(e))),
            }
        }

        let mut payload = vec![0u8; rh.size as usize];
        if let Err(e) = self.cursor.read_exact(&mut payload) {
            return Some(Err(HwpError::Io(e)));
        }

        Some(Ok(Record {
            header: rh,
            payload,
        }))
    }
}

/// Collect all records from raw bytes.
pub fn parse_records(data: &[u8]) -> Result<Vec<Record>> {
    RecordIter::new(data).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_header_decode() {
        // tag=17 (0x011), level=0 (0x000), size=28 (0x01C)
        // packed: (28 << 20) | (0 << 10) | 17 = 0x01C00011
        let h = RecordHeader::from_u32(0x01C00011);
        assert_eq!(h.tag, 17);
        assert_eq!(h.level, 0);
        assert_eq!(h.size, 28);
    }

    #[test]
    fn test_record_header_max_values() {
        // tag=1023, level=1023, size=4094 (not extended)
        let packed: u32 = (4094 << 20) | (1023 << 10) | 1023;
        let h = RecordHeader::from_u32(packed);
        assert_eq!(h.tag, 1023);
        assert_eq!(h.level, 1023);
        assert_eq!(h.size, 4094);
    }

    #[test]
    fn test_record_iter_basic() {
        // Create a minimal record: tag=17, level=0, size=4, payload=[1,2,3,4]
        let packed: u32 = (4 << 20) | (0 << 10) | 17;
        let mut data = packed.to_le_bytes().to_vec();
        data.extend_from_slice(&[1, 2, 3, 4]);

        let records: Vec<Record> = RecordIter::new(&data)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].header.tag, 17);
        assert_eq!(records[0].payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_record_iter_extended_size() {
        // Extended: size field = 0xFFF, followed by actual size u32
        let packed: u32 = (0xFFF << 20) | (0 << 10) | 42;
        let mut data = packed.to_le_bytes().to_vec();
        // Actual size = 6
        data.extend_from_slice(&6u32.to_le_bytes());
        data.extend_from_slice(&[10, 20, 30, 40, 50, 60]);

        let records: Vec<Record> = RecordIter::new(&data)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].header.tag, 42);
        assert_eq!(records[0].header.size, 6);
        assert_eq!(records[0].payload, vec![10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn test_record_iter_multiple() {
        let mut data = Vec::new();
        // Record 1: tag=16, level=0, size=2
        let packed1: u32 = (2 << 20) | (0 << 10) | 16;
        data.extend_from_slice(&packed1.to_le_bytes());
        data.extend_from_slice(&[0xAA, 0xBB]);
        // Record 2: tag=17, level=0, size=3
        let packed2: u32 = (3 << 20) | (0 << 10) | 17;
        data.extend_from_slice(&packed2.to_le_bytes());
        data.extend_from_slice(&[0xCC, 0xDD, 0xEE]);

        let records: Vec<Record> = RecordIter::new(&data)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].header.tag, 16);
        assert_eq!(records[1].header.tag, 17);
    }
}
