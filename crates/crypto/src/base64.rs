//! Minimal standard base64 (with padding), implemented locally so the crypto
//! surface does not depend on a third-party codec.

use crate::CryptoError;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn decode(text: &str) -> Result<Vec<u8>, CryptoError> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 4 != 0 {
        return Err(CryptoError::Base64(
            "length is not a multiple of four".into(),
        ));
    }
    let bytes = cleaned.as_bytes();
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    let mut index = 0;
    while index < bytes.len() {
        let block = &bytes[index..index + 4];
        let mut values = [0u32; 4];
        let mut padding = 0usize;
        for (slot, byte) in block.iter().enumerate() {
            if *byte == b'=' {
                if slot < 2 {
                    return Err(CryptoError::Base64("misplaced padding".into()));
                }
                padding += 1;
                values[slot] = 0;
            } else {
                values[slot] = decode_char(*byte)? as u32;
            }
        }
        let n = (values[0] << 18) | (values[1] << 12) | (values[2] << 6) | values[3];
        out.push(((n >> 16) & 0xFF) as u8);
        if padding < 2 {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if padding < 1 {
            out.push((n & 0xFF) as u8);
        }
        index += 4;
    }
    Ok(out)
}

fn decode_char(byte: u8) -> Result<u8, CryptoError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        other => Err(CryptoError::Base64(format!(
            "invalid character {:?}",
            other as char
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_rfc4648_vectors() {
        let cases: [(&str, &[u8]); 7] = [
            ("", b""),
            ("Zg==", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("Zm9vYg==", b"foob"),
            ("Zm9vYmE=", b"fooba"),
            ("Zm9vYmFy", b"foobar"),
        ];
        for (encoded, raw) in cases {
            assert_eq!(encode(raw), encoded);
            assert_eq!(decode(encoded).unwrap(), raw);
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert!(decode("abc").is_err());
        assert!(decode("ab=c").is_err());
        assert!(decode("****").is_err());
    }

    #[test]
    fn handles_binary_and_whitespace() {
        let raw: Vec<u8> = (0u8..=255).collect();
        let encoded = encode(&raw);
        let wrapped = encoded
            .as_bytes()
            .chunks(64)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(decode(&wrapped).unwrap(), raw);
    }
}
