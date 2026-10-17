//! The small subset of CBOR (RFC 8949) that `WebAuthn` needs: attestation objects and COSE
//! keys. Authenticators emit CTAP2 canonical CBOR, so only definite lengths are accepted;
//! floats, tags and indefinite lengths are refused rather than guessed at.

/// A decoded CBOR item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    /// Major types 0 and 1, as one signed range (`WebAuthn` never needs more than `i64`).
    Integer(i128),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
}

impl Value {
    /// The value under integer key `key` of a map.
    pub(crate) fn get_int(&self, key: i128) -> Option<&Value> {
        self.entries()?.iter().find(|(k, _)| *k == Value::Integer(key)).map(|(_, v)| v)
    }

    /// The value under text key `key` of a map.
    pub(crate) fn get_text(&self, key: &str) -> Option<&Value> {
        self.entries()?.iter().find(|(k, _)| matches!(k, Value::Text(text) if text == key)).map(|(_, v)| v)
    }

    fn entries(&self) -> Option<&[(Value, Value)]> {
        if let Value::Map(entries) = self { Some(entries) } else { None }
    }

    pub(crate) fn as_bytes(&self) -> Option<&[u8]> {
        if let Value::Bytes(bytes) = self { Some(bytes) } else { None }
    }

    pub(crate) fn as_int(&self) -> Option<i128> {
        if let Value::Integer(value) = self { Some(*value) } else { None }
    }
}

/// Nesting allowed before giving up (COSE keys and attestation objects need 3).
const MAX_DEPTH: usize = 16;

/// Decodes one item from the start of `input`; returns it and the number of bytes it used.
pub(crate) fn decode_prefix(input: &[u8]) -> Option<(Value, usize)> {
    let mut reader = Reader { input, position: 0 };
    let value = reader.item(0)?;
    Some((value, reader.position))
}

/// Decodes `input`, which must hold exactly one item.
pub(crate) fn decode(input: &[u8]) -> Option<Value> {
    let (value, used) = decode_prefix(input)?;
    (used == input.len()).then_some(value)
}

struct Reader<'a> {
    input: &'a [u8],
    position: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Option<&[u8]> {
        let end = self.position.checked_add(count)?;
        let bytes = self.input.get(self.position..end)?;
        self.position = end;
        Some(bytes)
    }

    /// The argument of a head with additional information `info`.
    fn argument(&mut self, info: u8) -> Option<u64> {
        Some(match info {
            0..=23 => u64::from(info),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().ok()?)),
            26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().ok()?)),
            27 => u64::from_be_bytes(self.take(8)?.try_into().ok()?),
            _ => return None,
        })
    }

    /// A length, bounded by what is left of the input so hostile lengths can't allocate.
    fn length(&mut self, info: u8) -> Option<usize> {
        let length = usize::try_from(self.argument(info)?).ok()?;
        (length <= self.input.len() - self.position).then_some(length)
    }

    fn item(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        let head = self.take(1)?[0];
        let (major, info) = (head >> 5, head & 0x1f);
        Some(match major {
            0 => Value::Integer(i128::from(self.argument(info)?)),
            1 => Value::Integer(-1 - i128::from(self.argument(info)?)),
            2 => {
                let length = self.length(info)?;
                Value::Bytes(self.take(length)?.to_vec())
            }
            3 => {
                let length = self.length(info)?;
                Value::Text(String::from_utf8(self.take(length)?.to_vec()).ok()?)
            }
            4 => {
                let length = self.length(info)?;
                let mut items = Vec::with_capacity(length);
                for _ in 0..length {
                    items.push(self.item(depth + 1)?);
                }
                Value::Array(items)
            }
            5 => {
                let length = self.length(info)?;
                let mut entries = Vec::with_capacity(length);
                for _ in 0..length {
                    let key = self.item(depth + 1)?;
                    let value = self.item(depth + 1)?;
                    entries.push((key, value));
                }
                Value::Map(entries)
            }
            7 => match info {
                20 => Value::Bool(false),
                21 => Value::Bool(true),
                22 => Value::Null,
                _ => return None,
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal encoder for building test inputs.
    pub(crate) fn encode(value: &Value) -> Vec<u8> {
        fn head(out: &mut Vec<u8>, major: u8, argument: u64) {
            let major = major << 5;
            match argument {
                0..=23 => out.push(major | u8::try_from(argument).unwrap()),
                24..=0xff => out.extend([major | 0x18, u8::try_from(argument).unwrap()]),
                0x100..=0xffff => {
                    out.push(major | 0x19);
                    out.extend(u16::try_from(argument).unwrap().to_be_bytes());
                }
                0x1_0000..=0xffff_ffff => {
                    out.push(major | 0x1a);
                    out.extend(u32::try_from(argument).unwrap().to_be_bytes());
                }
                _ => {
                    out.push(major | 0x1b);
                    out.extend(argument.to_be_bytes());
                }
            }
        }
        fn write(out: &mut Vec<u8>, value: &Value) {
            match value {
                Value::Integer(n) if *n >= 0 => head(out, 0, u64::try_from(*n).unwrap()),
                Value::Integer(n) => head(out, 1, u64::try_from(-1 - *n).unwrap()),
                Value::Bytes(bytes) => {
                    head(out, 2, bytes.len() as u64);
                    out.extend(bytes);
                }
                Value::Text(text) => {
                    head(out, 3, text.len() as u64);
                    out.extend(text.as_bytes());
                }
                Value::Array(items) => {
                    head(out, 4, items.len() as u64);
                    for item in items {
                        write(out, item);
                    }
                }
                Value::Map(entries) => {
                    head(out, 5, entries.len() as u64);
                    for (key, value) in entries {
                        write(out, key);
                        write(out, value);
                    }
                }
                Value::Bool(false) => out.push(0xf4),
                Value::Bool(true) => out.push(0xf5),
                Value::Null => out.push(0xf6),
            }
        }
        let mut out = Vec::new();
        write(&mut out, value);
        out
    }

    #[test]
    fn round_trips_the_types_webauthn_uses() {
        let value = Value::Map(vec![
            (Value::Integer(1), Value::Integer(2)),
            (Value::Integer(-7), Value::Integer(-257)),
            (Value::Text("fmt".to_owned()), Value::Text("none".to_owned())),
            (Value::Integer(-2), Value::Bytes(vec![7; 300])),
            (Value::Text("attStmt".to_owned()), Value::Map(vec![])),
            (Value::Text("list".to_owned()), Value::Array(vec![Value::Bool(true), Value::Null])),
        ]);
        let bytes = encode(&value);
        assert_eq!(decode(&bytes), Some(value.clone()));
        assert_eq!(value.get_int(-7).and_then(Value::as_int), Some(-257));
        assert_eq!(value.get_text("fmt"), Some(&Value::Text("none".to_owned())));
    }

    #[test]
    fn decode_prefix_reports_the_length_used() {
        let mut bytes = encode(&Value::Bytes(vec![1, 2, 3]));
        let used = bytes.len();
        bytes.extend([0xa0, 0xff]);
        assert_eq!(decode_prefix(&bytes), Some((Value::Bytes(vec![1, 2, 3]), used)));
        assert_eq!(decode(&bytes), None);
    }

    #[test]
    fn refuses_truncated_hostile_and_unsupported_input() {
        assert_eq!(decode(&[]), None);
        // A byte string claiming 4 GiB.
        assert_eq!(decode(&[0x5a, 0xff, 0xff, 0xff, 0xff]), None);
        // Indefinite-length byte string, a tag, a float.
        assert_eq!(decode(&[0x5f, 0xff]), None);
        assert_eq!(decode(&[0xc0, 0x00]), None);
        assert_eq!(decode(&[0xf9, 0x00, 0x00]), None);
        // Nesting deeper than allowed.
        assert_eq!(decode(&[0x81; 64]), None);
        // Invalid UTF-8 text.
        assert_eq!(decode(&[0x61, 0xff]), None);
    }
}
