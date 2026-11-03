//! RFC 8785-style canonical JSON, restricted to the shapes this protocol uses.
//!
//! Rules implemented:
//!
//! * object keys sorted by UTF-16 code units,
//! * no insignificant whitespace,
//! * strings escaped minimally (short escapes where they exist, `\u00xx`
//!   otherwise, non-ASCII emitted as raw UTF-8),
//! * numbers restricted to integers, because every protocol number is a count or
//!   a version and floats have no canonical cross-language form we need.

use serde_json::Value;

use crate::{ProtocolError, MAX_PAYLOAD_DEPTH};

pub fn canonical_json(value: &Value) -> Result<String, ProtocolError> {
    let mut out = String::new();
    write_value(value, &mut out, 0)?;
    Ok(out)
}

fn write_value(value: &Value, out: &mut String, depth: usize) -> Result<(), ProtocolError> {
    if depth > MAX_PAYLOAD_DEPTH {
        return Err(ProtocolError::Limits("payload nesting depth"));
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                out.push_str(&value.to_string());
            } else if let Some(value) = number.as_u64() {
                out.push_str(&value.to_string());
            } else {
                return Err(ProtocolError::NonIntegerNumber);
            }
        }
        Value::String(text) => write_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out, depth + 1)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_value(&map[*key], out, depth + 1)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_keys_recursively() {
        let value = json!({"b": 1, "a": {"z": true, "y": null}});
        assert_eq!(canonical_json(&value).unwrap(), r#"{"a":{"y":null,"z":true},"b":1}"#);
    }

    #[test]
    fn escapes_minimally() {
        let value = json!({"text": "a\"b\\c\nd\te\u{1}f"});
        assert_eq!(
            canonical_json(&value).unwrap(),
            "{\"text\":\"a\\\"b\\\\c\\nd\\te\\u0001f\"}"
        );
    }

    #[test]
    fn keeps_utf8_raw() {
        let value = json!({"text": "سلام دنیا"});
        assert_eq!(canonical_json(&value).unwrap(), "{\"text\":\"سلام دنیا\"}");
    }

    #[test]
    fn rejects_float_numbers() {
        let value = json!({"n": 1.5});
        assert!(matches!(
            canonical_json(&value),
            Err(ProtocolError::NonIntegerNumber)
        ));
    }

    #[test]
    fn rejects_excessive_nesting() {
        let mut value = json!(null);
        for _ in 0..MAX_PAYLOAD_DEPTH + 2 {
            value = json!([value]);
        }
        assert!(matches!(
            canonical_json(&value),
            Err(ProtocolError::Limits(_))
        ));
    }

    #[test]
    fn is_insertion_order_independent() {
        let a = json!({"one": 1, "two": 2, "three": [3, 2, 1]});
        let b = json!({"three": [3, 2, 1], "two": 2, "one": 1});
        assert_eq!(canonical_json(&a).unwrap(), canonical_json(&b).unwrap());
    }
}
