//! Raw decoder for the 26.2 `ClientboundCooldownPacket` payload.
//!
//! Keep this independent of Azalea's pinned `ClientboundCooldown`, which
//! incorrectly models the group Identifier as an ItemKind.

use std::io::Cursor;

use azalea_buf::{AzBuf, AzBufVar};
use azalea_registry::identifier::Identifier;

#[derive(Debug, PartialEq, Eq)]
pub struct Cooldown {
    pub group: Identifier,
    /// Signed protocol VarInt; state consumers decide how to handle negatives.
    pub duration: i32,
}

/// Decodes exactly `Identifier cooldownGroup, VarInt duration` (payload only).
pub fn decode_payload(payload: &[u8]) -> Result<Cooldown, &'static str> {
    let mut cur = Cursor::new(payload);
    let group = String::azalea_read(&mut cur).map_err(|_| "invalid cooldown group")?;
    if !valid_identifier(&group) {
        return Err("invalid cooldown group");
    }
    let duration = i32::azalea_read_var(&mut cur).map_err(|_| "invalid cooldown duration")?;
    if cur.position() as usize != payload.len() {
        return Err("trailing cooldown payload bytes");
    }
    Ok(Cooldown {
        group: Identifier::new(group),
        duration,
    })
}

fn valid_identifier(value: &str) -> bool {
    let valid = |part: &str, allowed: &str| {
        !part.is_empty()
            && part.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || allowed.contains(b as char)
            })
    };
    match value.split_once(':') {
        Some((namespace, path)) => {
            !path.contains(':') && valid(namespace, "_.-") && valid(path, "_./-")
        }
        None => valid(value, "_./-"),
    }
}

#[cfg(test)]
mod tests {
    use pomme_protocol::wire;

    use super::*;

    fn payload(group: &str, duration: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        wire::write_varint(&mut bytes, group.len() as u32);
        bytes.extend_from_slice(group.as_bytes());
        wire::write_varint(&mut bytes, duration as u32);
        bytes
    }

    #[test]
    fn native_shared_group_and_zero_duration() {
        let a = decode_payload(&payload("test:shared", 40)).unwrap();
        let b = decode_payload(&payload("test:shared", 0)).unwrap();
        assert_eq!(a.group, Identifier::new("test:shared"));
        assert_eq!(a.duration, 40);
        assert_eq!(b.group, a.group);
        assert_eq!(b.duration, 0);
    }

    #[test]
    fn signed_duration_is_preserved() {
        let cooldown = decode_payload(&payload("test:signed", -1)).unwrap();
        assert_eq!(cooldown.duration, -1);
    }

    #[test]
    fn rejects_invalid_or_incomplete_payloads() {
        assert!(decode_payload(&[0x80]).is_err()); // truncated Identifier length
        assert!(decode_payload(&payload("not valid", 1)).is_err());
        let mut truncated_duration = payload("test:group", 1);
        truncated_duration.pop();
        assert!(decode_payload(&truncated_duration).is_err());
        let mut trailing = payload("test:group", 1);
        trailing.push(0);
        assert!(decode_payload(&trailing).is_err());
    }
}
