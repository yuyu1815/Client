mod extra;
mod primitives;

use std::backtrace::Backtrace;
use std::io::{self, Cursor, Write};

use thiserror::Error;

/// A trait that's implemented on types that are used by the Minecraft protocol.
pub trait AzBuf
where
    Self: Sized,
{
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError>;
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()>;
}

/// Used for types that have an alternative variable-length encoding.
///
/// This mostly exists for varints.
pub trait AzBufVar
where
    Self: Sized,
{
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError>;
    fn azalea_write_var(&self, buf: &mut impl Write) -> io::Result<()>;
}

/// Used for types that have some configurable limit.
///
/// For example, the implementation of this on `String` limits the maximum
/// length of the string.
///
/// This exists partially as an anti-abuse mechanism in Minecraft, so there is
/// no limited write function.
pub trait AzBufLimited
where
    Self: Sized,
{
    fn azalea_read_limited(buf: &mut Cursor<&[u8]>, limit: u32) -> Result<Self, BufReadError>;
}

#[derive(Debug, Error)]
pub enum BufReadError {
    #[error("Invalid VarInt")]
    InvalidVarInt,
    #[error("Invalid VarLong")]
    InvalidVarLong,
    #[error("Error reading bytes")]
    CouldNotReadBytes,
    #[error(
        "The received encoded string buffer length is longer than maximum allowed ({length} > {max_length})"
    )]
    StringLengthTooLong { length: u32, max_length: u32 },
    #[error("The received Vec length is longer than maximum allowed ({length} > {max_length})")]
    VecLengthTooLong { length: u32, max_length: u32 },
    #[error("{source}")]
    Io {
        #[from]
        #[backtrace]
        source: io::Error,
    },
    #[error("Invalid UTF-8: {bytes:?} (lossy: {lossy:?})")]
    InvalidUtf8 {
        bytes: Vec<u8>,
        lossy: String,
        // backtrace: Backtrace,
    },
    #[error("Unexpected enum variant {id}")]
    UnexpectedEnumVariant { id: i32 },
    #[error("Unexpected enum variant {id}")]
    UnexpectedStringEnumVariant { id: String },
    #[error("Tried to read {attempted_read} bytes but there were only {actual_read}")]
    UnexpectedEof {
        attempted_read: usize,
        actual_read: usize,
        backtrace: Backtrace,
    },
    #[error("{0}")]
    Custom(String),
    #[cfg(feature = "serde_json")]
    #[error("{source}")]
    Deserialization {
        #[from]
        #[backtrace]
        source: serde_json::Error,
    },
    #[error("{source}")]
    Nbt {
        #[from]
        #[backtrace]
        source: simdnbt::Error,
    },
    #[error("{source}")]
    DeserializeNbt {
        #[from]
        #[backtrace]
        source: simdnbt::DeserializeError,
    },
}

pub(crate) fn read_bytes<'a>(
    buf: &'a mut Cursor<&[u8]>,
    length: usize,
) -> Result<&'a [u8], BufReadError> {
    if length > (buf.get_ref().len() - buf.position() as usize) {
        return Err(BufReadError::UnexpectedEof {
            attempted_read: length,
            actual_read: buf.get_ref().len() - buf.position() as usize,
            backtrace: Backtrace::capture(),
        });
    }
    let initial_position = buf.position() as usize;
    buf.set_position(buf.position() + length as u64);
    let data = &buf.get_ref()[initial_position..initial_position + length];
    Ok(data)
}

pub(crate) fn read_utf_with_len<'a>(
    buf: &'a mut Cursor<&[u8]>,
    max_length: u32,
) -> Result<&'a str, BufReadError> {
    let length = u32::azalea_read_var(buf)?;
    let max_bytes = max_length.saturating_mul(3);
    if length > max_bytes {
        return Err(BufReadError::StringLengthTooLong {
            length,
            max_length: max_bytes,
        });
    }

    let buffer = read_bytes(buf, length as usize)?;
    let string = std::str::from_utf8(buffer).map_err(|_| BufReadError::InvalidUtf8 {
        bytes: buffer.to_vec(),
        lossy: String::from_utf8_lossy(buffer).to_string(),
        // backtrace: Backtrace::capture(),
    })?;
    let utf16_length = string.encode_utf16().count();
    if utf16_length > max_length as usize {
        return Err(BufReadError::StringLengthTooLong {
            length: u32::try_from(utf16_length).unwrap_or(u32::MAX),
            max_length,
        });
    }

    Ok(string)
}

pub(crate) fn write_utf_with_len(
    buf: &mut impl Write,
    string: &str,
    max_len: u32,
) -> io::Result<()> {
    let utf16_length = string.encode_utf16().count();
    let max_bytes = max_len.saturating_mul(3);
    if utf16_length > max_len as usize || string.len() > max_bytes as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("String too big ({utf16_length} UTF-16 units, max {max_len})"),
        ));
    }
    (string.len() as u32).azalea_write_var(buf)?;
    buf.write_all(string.as_bytes())
}

#[cfg(test)]
mod utf_string_tests {
    use super::*;
    use crate::{AzBuf, AzBufLimited};

    fn wire(bytes: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        (bytes.len() as u32).azalea_write_var(&mut result).unwrap();
        result.extend_from_slice(bytes);
        result
    }

    fn read(input: &[u8]) -> Result<String, BufReadError> {
        String::azalea_read(&mut Cursor::new(input))
    }

    #[test]
    fn utf16_boundary_read_and_write() {
        for (text, units, bytes) in [
            ("a".repeat(32767), 32767, 32767),
            ("\u{0800}".repeat(32767), 32767, 98301),
            (format!("{}a", "\u{1f600}".repeat(16383)), 32767, 65533),
        ] {
            assert_eq!(text.encode_utf16().count(), units);
            assert_eq!(text.len(), bytes);
            assert_eq!(read(&wire(text.as_bytes())).unwrap(), text);
            let mut output = Vec::new();
            write_utf_with_len(&mut output, &text, 32767).unwrap();
            assert_eq!(output, wire(text.as_bytes()));
        }
        for text in [
            "a".repeat(32768),
            "\u{0800}".repeat(32768),
            "\u{1f600}".repeat(16384),
        ] {
            assert!(read(&wire(text.as_bytes())).is_err());
            let mut output = vec![0xaa];
            assert!(write_utf_with_len(&mut output, &text, 32767).is_err());
            assert_eq!(output, [0xaa]);
        }
    }

    #[test]
    fn limited_box_and_malformed_inputs() {
        let accepted = wire("\u{1f600}".as_bytes());
        assert_eq!(
            String::azalea_read_limited(&mut Cursor::new(&accepted), 2).unwrap(),
            "\u{1f600}"
        );
        let mut custom_output = Vec::new();
        write_utf_with_len(&mut custom_output, "ab", 2).unwrap();
        assert_eq!(custom_output, wire(b"ab"));
        let mut rejected_output = vec![0xaa];
        assert!(write_utf_with_len(&mut rejected_output, "ab", 1).is_err());
        assert_eq!(rejected_output, [0xaa]);
        assert!(String::azalea_read_limited(&mut Cursor::new(&accepted), 1).is_err());
        assert_eq!(
            Box::<str>::azalea_read(&mut Cursor::new(&accepted))
                .unwrap()
                .as_ref(),
            "\u{1f600}"
        );
        let mut boxed_output = Vec::new();
        "\u{1f600}"
            .to_owned()
            .into_boxed_str()
            .azalea_write(&mut boxed_output)
            .unwrap();
        assert_eq!(boxed_output, accepted);
        assert!(read(&[5, b'a']).is_err()); // short payload
        assert!(read(&[0xfe, 0xff, 0x05]).is_err()); // oversized declared byte length
        assert!(read(&[0xff, 0xff, 0xff, 0xff, 0x0f]).is_err()); // negative VarInt
        assert!(read(&[1, 0xff]).is_err()); // strict UTF-8 remains strict
    }
}
