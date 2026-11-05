//! Hex helpers shared with the protocol crate.

use crate::CryptoError;

pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn from_hex(text: &str) -> Result<Vec<u8>, CryptoError> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(CryptoError::Key("hex string has odd length".into()));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = nibble(bytes[index])?;
        let low = nibble(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Ok(out)
}

fn nibble(byte: u8) -> Result<u8, CryptoError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        other => Err(CryptoError::Key(format!(
            "invalid hex character {:?}",
            other as char
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let raw = [0u8, 1, 15, 16, 127, 128, 255];
        assert_eq!(to_hex(&raw), "00010f107f80ff");
        assert_eq!(from_hex("00010F107F80FF").unwrap(), raw);
    }

    #[test]
    fn rejects_malformed() {
        assert!(from_hex("abc").is_err());
        assert!(from_hex("zz").is_err());
    }
}
