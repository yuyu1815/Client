use std::io::{self, Cursor, Read, Write};

use byteorder::{BE, ReadBytesExt, WriteBytesExt};
use tracing::warn;

use crate::{AzBuf, AzBufVar, BufReadError};

impl AzBuf for () {
    fn azalea_read(_buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(())
    }
    fn azalea_write(&self, _buf: &mut impl Write) -> io::Result<()> {
        Ok(())
    }
}

impl AzBuf for i32 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_i32::<BE>()?)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_i32::<BE>(*self)
    }
}

impl AzBufVar for i32 {
    /// Read a single varint from the reader and return the value
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        // fast varint impl based on https://github.com/luojia65/mc-varint/blob/master/src/lib.rs#L67
        let mut buffer = [0];
        let mut ans = 0;
        for i in 0..5 {
            buf.read_exact(&mut buffer)?;
            ans |= ((buffer[0] & 0b0111_1111) as i32) << (7 * i);
            if buffer[0] & 0b1000_0000 == 0 {
                break;
            }
            if i == 4 {
                return Err(BufReadError::InvalidVarInt);
            }
        }
        Ok(ans)
    }

    fn azalea_write_var(&self, buf: &mut impl Write) -> io::Result<()> {
        let mut buffer = [0];
        let mut value = *self;
        if value == 0 {
            buf.write_all(&buffer)?;
        }
        while value != 0 {
            buffer[0] = (value & 0b0111_1111) as u8;
            value = (value >> 7) & (i32::MAX >> 6);
            if value != 0 {
                buffer[0] |= 0b1000_0000;
            }
            buf.write_all(&buffer)?;
        }
        Ok(())
    }
}

impl AzBufVar for i64 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let mut buffer = [0];
        let mut ans = 0;
        for i in 0..10 {
            buf.read_exact(&mut buffer)
                .map_err(|_| BufReadError::InvalidVarLong)?;
            ans |= ((buffer[0] & 0b0111_1111) as i64) << (7 * i);
            if buffer[0] & 0b1000_0000 == 0 {
                break;
            }
            if i == 9 {
                return Err(BufReadError::InvalidVarLong);
            }
        }
        Ok(ans)
    }

    fn azalea_write_var(&self, buf: &mut impl Write) -> io::Result<()> {
        let mut buffer = [0];
        let mut value = *self;
        if value == 0 {
            buf.write_all(&buffer).unwrap();
        }
        while value != 0 {
            buffer[0] = (value & 0b0111_1111) as u8;
            value = (value >> 7) & (i64::MAX >> 6);
            if value != 0 {
                buffer[0] |= 0b1000_0000;
            }
            buf.write_all(&buffer)?;
        }
        Ok(())
    }
}

impl AzBufVar for u64 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        i64::azalea_read_var(buf).map(|i| i as u64)
    }
    fn azalea_write_var(&self, buf: &mut impl Write) -> io::Result<()> {
        i64::azalea_write_var(&(*self as i64), buf)
    }
}

impl AzBuf for u32 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(i32::azalea_read(buf)? as u32)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        i32::azalea_write(&(*self as i32), buf)
    }
}

impl AzBufVar for u32 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(i32::azalea_read_var(buf)? as u32)
    }
    fn azalea_write_var(&self, buf: &mut impl Write) -> io::Result<()> {
        i32::azalea_write_var(&(*self as i32), buf)
    }
}

impl AzBuf for u16 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        i16::azalea_read(buf).map(|i| i as u16)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        i16::azalea_write(&(*self as i16), buf)
    }
}

impl AzBuf for i16 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_i16::<BE>()?)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_i16::<BE>(*self)
    }
}

impl AzBufVar for u16 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(i32::azalea_read_var(buf)? as u16)
    }
    fn azalea_write_var(&self, buf: &mut impl Write) -> io::Result<()> {
        i32::azalea_write_var(&(*self as i32), buf)
    }
}

impl AzBuf for i64 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_i64::<BE>()?)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_i64::<BE>(*self)
    }
}

impl AzBuf for u64 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        i64::azalea_read(buf).map(|i| i as u64)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_u64::<BE>(*self)
    }
}

impl AzBuf for bool {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let byte = u8::azalea_read(buf)?;
        if byte > 1 {
            warn!("Boolean value was not 0 or 1, but {byte}");
        }
        Ok(byte != 0)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        let byte = u8::from(*self);
        byte.azalea_write(buf)
    }
}

impl AzBuf for u8 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_u8()?)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_u8(*self)
    }
}

impl AzBuf for i8 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        u8::azalea_read(buf).map(|i| i as i8)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        (*self as u8).azalea_write(buf)
    }
}

impl AzBuf for f32 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_f32::<BE>()?)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_f32::<BE>(*self)
    }
}

impl AzBuf for f64 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_f64::<BE>()?)
    }
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        buf.write_f64::<BE>(*self)
    }
}

#[cfg(test)]
mod varint_tests {
    use super::*;

    fn check_i32(value: i32) {
        let mut bytes = Vec::new();
        value.azalea_write_var(&mut bytes).unwrap();
        let mut cursor = Cursor::new(bytes.as_slice());
        assert_eq!(i32::azalea_read_var(&mut cursor).unwrap(), value);
        assert_eq!(cursor.position() as usize, bytes.len());
    }

    fn check_i64(value: i64) {
        let mut bytes = Vec::new();
        value.azalea_write_var(&mut bytes).unwrap();
        let mut cursor = Cursor::new(bytes.as_slice());
        assert_eq!(i64::azalea_read_var(&mut cursor).unwrap(), value);
        assert_eq!(cursor.position() as usize, bytes.len());
    }

    #[test]
    fn signed_and_unsigned_boundaries_round_trip() {
        for value in [0, i32::MIN, i32::MAX, -1] {
            check_i32(value);
            let unsigned = value as u32;
            let mut bytes = Vec::new();
            unsigned.azalea_write_var(&mut bytes).unwrap();
            let mut cursor = Cursor::new(bytes.as_slice());
            assert_eq!(u32::azalea_read_var(&mut cursor).unwrap(), unsigned);
            assert_eq!(cursor.position() as usize, bytes.len());
        }
        for value in [0, i64::MIN, i64::MAX, -1] {
            check_i64(value);
            let unsigned = value as u64;
            let mut bytes = Vec::new();
            unsigned.azalea_write_var(&mut bytes).unwrap();
            let mut cursor = Cursor::new(bytes.as_slice());
            assert_eq!(u64::azalea_read_var(&mut cursor).unwrap(), unsigned);
            assert_eq!(cursor.position() as usize, bytes.len());
        }
    }

    #[test]
    fn accepts_nonminimal_and_unused_high_payload_bits() {
        for input in [&[0x80, 0x00][..], &[0xff, 0xff, 0xff, 0xff, 0x7f][..]] {
            let mut cursor = Cursor::new(input);
            assert!(i32::azalea_read_var(&mut cursor).is_ok());
            assert_eq!(cursor.position() as usize, input.len());
            let mut cursor = Cursor::new(input);
            assert!(u32::azalea_read_var(&mut cursor).is_ok());
            assert_eq!(cursor.position() as usize, input.len());
        }
        for input in [
            &[0x80, 0x00][..],
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f][..],
        ] {
            let mut cursor = Cursor::new(input);
            assert!(i64::azalea_read_var(&mut cursor).is_ok());
            assert_eq!(cursor.position() as usize, input.len());
            let mut cursor = Cursor::new(input);
            assert!(u64::azalea_read_var(&mut cursor).is_ok());
            assert_eq!(cursor.position() as usize, input.len());
        }
    }

    #[test]
    fn rejects_continuation_at_cap_without_reading_next_byte() {
        let int_input = [0x80; 6];
        for result in [
            i32::azalea_read_var(&mut Cursor::new(&int_input[..])),
            i32::azalea_read_var(&mut Cursor::new(&int_input[..5])),
        ] {
            assert!(matches!(result, Err(BufReadError::InvalidVarInt)));
        }
        for result in [
            u32::azalea_read_var(&mut Cursor::new(&int_input[..])),
            u32::azalea_read_var(&mut Cursor::new(&int_input[..5])),
        ] {
            assert!(matches!(result, Err(BufReadError::InvalidVarInt)));
        }
        for result in [
            i64::azalea_read_var(&mut Cursor::new(&[0x80; 11][..])),
            i64::azalea_read_var(&mut Cursor::new(&[0x80; 10][..])),
        ] {
            assert!(matches!(result, Err(BufReadError::InvalidVarLong)));
        }
        for result in [
            u64::azalea_read_var(&mut Cursor::new(&[0x80; 11][..])),
            u64::azalea_read_var(&mut Cursor::new(&[0x80; 10][..])),
        ] {
            assert!(matches!(result, Err(BufReadError::InvalidVarLong)));
        }

        for input in [&int_input[..], &int_input[..5]] {
            let mut cursor = Cursor::new(input);
            let result = i32::azalea_read_var(&mut cursor);
            assert!(matches!(result, Err(BufReadError::InvalidVarInt)));
            assert_eq!(cursor.position(), 5);
            let mut cursor = Cursor::new(input);
            let result = u32::azalea_read_var(&mut cursor);
            assert!(matches!(result, Err(BufReadError::InvalidVarInt)));
            assert_eq!(cursor.position(), 5);
        }
        let long_input = [0x80; 11];
        for input in [&long_input[..], &long_input[..10]] {
            let mut cursor = Cursor::new(input);
            let result = i64::azalea_read_var(&mut cursor);
            assert!(matches!(result, Err(BufReadError::InvalidVarLong)));
            assert_eq!(cursor.position(), 10);
            let mut cursor = Cursor::new(input);
            let result = u64::azalea_read_var(&mut cursor);
            assert!(matches!(result, Err(BufReadError::InvalidVarLong)));
            assert_eq!(cursor.position(), 10);
        }
    }

    #[test]
    fn preserves_truncation_error_kinds_before_cap() {
        assert!(matches!(
            i32::azalea_read_var(&mut Cursor::new(&[0x80][..])),
            Err(BufReadError::Io { .. })
        ));
        assert!(matches!(
            u32::azalea_read_var(&mut Cursor::new(&[0x80][..])),
            Err(BufReadError::Io { .. })
        ));
        assert!(matches!(
            i64::azalea_read_var(&mut Cursor::new(&[0x80][..])),
            Err(BufReadError::InvalidVarLong)
        ));
        assert!(matches!(
            u64::azalea_read_var(&mut Cursor::new(&[0x80][..])),
            Err(BufReadError::InvalidVarLong)
        ));
    }
}
