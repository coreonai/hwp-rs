use std::io::{Read, Seek};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::error::{HwpError, Result};
use crate::model::FileHeader;

const SIGNATURE: &[u8] = b"HWP Document File";
const SIGNATURE_LEN: usize = 32; // padded with zeros to 32 bytes

/// Parse the FileHeader stream from an OLE2 compound file.
pub fn parse_file_header<R: Read + Seek>(cfb: &mut cfb::CompoundFile<R>) -> Result<FileHeader> {
    let mut stream = cfb.open_stream("/FileHeader")?;
    let mut sig_buf = [0u8; SIGNATURE_LEN];
    stream.read_exact(&mut sig_buf)?;

    if &sig_buf[..SIGNATURE.len()] != SIGNATURE {
        let sig_str = String::from_utf8_lossy(&sig_buf[..SIGNATURE.len()]).to_string();
        return Err(HwpError::InvalidSignature(sig_str));
    }

    // Version: u32 LE — major.minor.build.revision stored as MM.mm.pp.rr
    let version_raw = stream.read_u32::<LittleEndian>()?;
    let major = ((version_raw >> 24) & 0xFF) as u8;
    let minor = ((version_raw >> 16) & 0xFF) as u8;
    let build = ((version_raw >> 8) & 0xFF) as u8;
    let revision = (version_raw & 0xFF) as u8;

    // Property flags: u32 LE
    let flags = stream.read_u32::<LittleEndian>()?;

    Ok(FileHeader {
        version: (major, minor, build, revision),
        compressed: flags & (1 << 0) != 0,
        encrypted: flags & (1 << 1) != 0,
        distributed: flags & (1 << 2) != 0,
        script: flags & (1 << 3) != 0,
        drm: flags & (1 << 4) != 0,
        xml_template: flags & (1 << 5) != 0,
        history: flags & (1 << 6) != 0,
        cert_signed: flags & (1 << 7) != 0,
        cert_encrypted: flags & (1 << 8) != 0,
        cert_drm: flags & (1 << 9) != 0,
        ccl: flags & (1 << 10) != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_constant() {
        assert_eq!(SIGNATURE, b"HWP Document File");
        assert_eq!(SIGNATURE.len(), 17);
    }
}
