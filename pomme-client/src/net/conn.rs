//! A Minecraft protocol connection over either transport.
//!
//! Replaces `azalea_protocol::connect::Connection`, which is pinned to a TCP
//! stream's owned halves and cannot be rebuilt over another transport because
//! its half wrappers carry private `PhantomData`. The framing beneath it is
//! already transport-generic, so this owns only the struct layer.
//!
//! There is also no connection phase here. Azalea makes it a type parameter and
//! re-types the whole connection on every transition; the packet passed at each
//! call already fixes the phase, and dropping the marker makes the mid-session
//! reconfiguration excursion ordinary calls on one object.

use std::fmt::Debug;
use std::io::{self, Cursor};

use azalea_buf::AzBufVar;
use azalea_crypto::{Aes128CfbDec, Aes128CfbEnc};
use azalea_protocol::packets::{Packet, ProtocolPacket};
use azalea_protocol::read::{
    DecompressionError, FrameSplitterError, MAXIMUM_UNCOMPRESSED_LENGTH, ReadPacketError,
    deserialize_packet,
};
use azalea_protocol::write::{serialize_packet, write_raw_packet};
use flate2::{Decompress, FlushDecompress, Status};
use tokio::io::{AsyncReadExt, ReadHalf, SimplexStream, WriteHalf};
use tokio::net::TcpStream;

use super::stream::{NetReader, NetWriter};

const MAX_FRAME_LENGTH: usize = (1 << 21) - 1;
const READ_CHUNK_SIZE: usize = 8192;

pub struct RawReader {
    stream: NetReader,
    /// A fully validated frame body in progress. No bytes from the next frame
    /// are read until this body has been returned.
    frame: Vec<u8>,
    frame_length: Option<usize>,
    /// Persistent prefix state makes cancellation during a partial prefix safe.
    prefix: [u8; 3],
    prefix_len: usize,
    failed: bool,
    compression_threshold: Option<u32>,
    dec_cipher: Option<Aes128CfbDec>,
}

pub struct RawWriter {
    stream: NetWriter,
    compression_threshold: Option<u32>,
    enc_cipher: Option<Aes128CfbEnc>,
}

/// Held as two fields rather than behind accessors so the game loop can read
/// and write in one `select!` on disjoint borrows.
pub struct Conn {
    pub reader: RawReader,
    pub writer: RawWriter,
}

fn frame_length_error(size: usize) -> Box<ReadPacketError> {
    Box::new(ReadPacketError::FrameSplitter {
        source: FrameSplitterError::BadLength {
            max: MAX_FRAME_LENGTH,
            size,
        },
    })
}

/// Decode only an already-received prefix. A missing terminator is partial
/// input; a continuation bit on byte three is an invalid VarInt21 prefix.
fn decode_frame_length(prefix: &[u8]) -> Result<Option<usize>, Box<ReadPacketError>> {
    let mut value = 0usize;
    for (index, &byte) in prefix.iter().enumerate() {
        value |= usize::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            if value == 0 || value > MAX_FRAME_LENGTH {
                return Err(frame_length_error(value));
            }
            return Ok(Some(value));
        }
        if index == 2 {
            return Err(frame_length_error(MAX_FRAME_LENGTH + 1));
        }
    }
    Ok(None)
}

fn compression_io_error(message: &'static str) -> Box<ReadPacketError> {
    Box::new(ReadPacketError::Decompress {
        source: DecompressionError::Io {
            source: io::Error::new(io::ErrorKind::InvalidData, message),
        },
    })
}

fn reader_failed_error() -> Box<ReadPacketError> {
    Box::new(ReadPacketError::from(io::Error::new(
        io::ErrorKind::InvalidData,
        "reader is unusable after previous read error",
    )))
}

fn validate_compressed_size(n: u32, threshold: u32) -> Result<usize, Box<ReadPacketError>> {
    if n < threshold {
        return Err(Box::new(ReadPacketError::Decompress {
            source: DecompressionError::BelowCompressionThreshold { size: n, threshold },
        }));
    }
    if n > MAXIMUM_UNCOMPRESSED_LENGTH {
        return Err(Box::new(ReadPacketError::Decompress {
            source: DecompressionError::AboveCompressionThreshold {
                size: n,
                maximum: MAXIMUM_UNCOMPRESSED_LENGTH,
            },
        }));
    }
    Ok(n as usize)
}

/// Decode Azalea's compression envelope with output bounded to declared size
/// plus one detection byte. The vector's length/capacity is fixed before
/// inflate; it never grows in response to compressed input.
fn decode_compressed(frame: &[u8], threshold: u32) -> Result<Box<[u8]>, Box<ReadPacketError>> {
    let mut cursor = Cursor::new(frame);
    let declared = u32::azalea_read_var(&mut cursor).map_err(|source| {
        Box::new(ReadPacketError::Decompress {
            source: DecompressionError::LengthReadError { source },
        })
    })?;
    let offset = cursor.position() as usize;

    if declared == 0 {
        return Ok(frame[offset..].into());
    }

    let expected = validate_compressed_size(declared, threshold)?;
    let compressed = &frame[offset..];
    let output_limit = expected + 1;
    let mut output = vec![0u8; output_limit];
    let mut inflater = Decompress::new(true);
    let mut total_in = 0u64;
    let mut total_out = 0u64;

    loop {
        let in_offset = usize::try_from(total_in).unwrap_or(usize::MAX);
        let out_offset = usize::try_from(total_out).unwrap_or(usize::MAX);
        if in_offset > compressed.len() || out_offset > output_limit {
            return Err(compression_io_error("invalid zlib decoder offsets"));
        }
        let before_in = total_in;
        let before_out = total_out;
        let status = inflater
            .decompress(
                &compressed[in_offset..],
                &mut output[out_offset..],
                FlushDecompress::Finish,
            )
            .map_err(|_| compression_io_error("invalid zlib stream"))?;
        total_in = inflater.total_in();
        total_out = inflater.total_out();

        if total_out > expected as u64 {
            return Err(compression_io_error(
                "zlib output exceeds declared uncompressed length",
            ));
        }
        if status == Status::StreamEnd {
            if total_out != expected as u64 {
                return Err(compression_io_error(
                    "zlib output does not match declared uncompressed length",
                ));
            }
            output.truncate(expected);
            return Ok(output.into_boxed_slice());
        }
        if total_in == before_in && total_out == before_out {
            return Err(compression_io_error("incomplete zlib stream"));
        }
        if total_out as usize == output_limit {
            // `expected + 1` output means the declared size was exceeded.
            return Err(compression_io_error(
                "zlib output exceeds declared uncompressed length",
            ));
        }
    }
}

impl RawReader {
    /// Reads one VarInt21 frame, decrypts it with the persistent CFB8 state,
    /// then optionally decompresses it. Once an actual read returns an error,
    /// the reader is terminal; a canceled future does not reach this wrapper's
    /// error path and leaves the persistent partial state resumable.
    pub async fn read(&mut self) -> Result<Box<[u8]>, Box<ReadPacketError>> {
        if self.failed {
            return Err(reader_failed_error());
        }
        let result = self.read_inner().await;
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// `AsyncReadExt::read` is cancellation safe; after it completes, cipher
    /// and parser state are updated before the next await, and all partial
    /// parser state lives in `self`.
    async fn read_inner(&mut self) -> Result<Box<[u8]>, Box<ReadPacketError>> {
        loop {
            if self.frame_length.is_none() {
                let mut byte = [0u8; 1];
                let count = self.stream.read(&mut byte).await.map_err(|error| {
                    Box::new(ReadPacketError::from(error)) as Box<ReadPacketError>
                })?;
                if count == 0 {
                    return Err(Box::new(ReadPacketError::ConnectionClosed));
                }
                if let Some(cipher) = &mut self.dec_cipher {
                    azalea_crypto::decrypt_packet(cipher, &mut byte[..count]);
                }

                let index = self.prefix_len;
                self.prefix[index] = byte[0];
                self.prefix_len += 1;
                if let Some(length) = decode_frame_length(&self.prefix[..self.prefix_len])? {
                    self.frame_length = Some(length);
                    self.frame = Vec::with_capacity(length);
                }
                continue;
            }

            let expected = self.frame_length.expect("checked above");
            let remaining = expected - self.frame.len();
            if remaining != 0 {
                let mut chunk = [0u8; READ_CHUNK_SIZE];
                let read_len = remaining.min(READ_CHUNK_SIZE);
                let count = self
                    .stream
                    .read(&mut chunk[..read_len])
                    .await
                    .map_err(|error| {
                        Box::new(ReadPacketError::from(error)) as Box<ReadPacketError>
                    })?;
                if count == 0 {
                    return Err(Box::new(ReadPacketError::ConnectionClosed));
                }
                if let Some(cipher) = &mut self.dec_cipher {
                    azalea_crypto::decrypt_packet(cipher, &mut chunk[..count]);
                }
                self.frame.extend_from_slice(&chunk[..count]);
                continue;
            }

            let frame = std::mem::take(&mut self.frame);
            self.frame_length = None;
            self.prefix = [0; 3];
            self.prefix_len = 0;
            return match self.compression_threshold {
                Some(threshold) => decode_compressed(&frame, threshold),
                None => Ok(frame.into_boxed_slice()),
            };
        }
    }
}

impl RawWriter {
    /// Writes one already-serialized frame, compressing and encrypting it.
    pub async fn write(&mut self, frame: &[u8]) -> io::Result<()> {
        write_raw_packet(
            frame,
            &mut self.stream,
            self.compression_threshold,
            &mut self.enc_cipher,
        )
        .await
    }
}

/// One side of an in-process connection.
pub struct MemoryEnd {
    pub rx: ReadHalf<SimplexStream>,
    pub tx: WriteHalf<SimplexStream>,
}

/// Pipes an integrated server and the client to each other, returning the
/// client's end and the server's.
///
/// Two independent pipes rather than a duplex pair, so neither direction waits
/// on a lock held by the other. The client-to-server pipe is oversized because
/// the game loop writes from the task that reads (see the TODO in `game_loop`).
#[allow(dead_code, reason = "only a singleplayer build opens a world")]
pub fn memory_pipes() -> (MemoryEnd, MemoryEnd) {
    const TO_CLIENT: usize = 1024 * 1024;
    const TO_SERVER: usize = 256 * 1024;

    let (client_rx, server_tx) = tokio::io::simplex(TO_CLIENT);
    let (server_rx, client_tx) = tokio::io::simplex(TO_SERVER);
    (
        MemoryEnd {
            rx: client_rx,
            tx: client_tx,
        },
        MemoryEnd {
            rx: server_rx,
            tx: server_tx,
        },
    )
}

impl Conn {
    pub fn is_encrypted(&self) -> bool {
        self.writer.enc_cipher.is_some()
    }

    pub fn from_tcp(stream: TcpStream) -> Self {
        let (read, write) = stream.into_split();
        Self::new(NetReader::Tcp(read), NetWriter::Tcp(write))
    }

    pub fn from_memory(end: MemoryEnd) -> Self {
        Self::new(NetReader::Memory(end.rx), NetWriter::Memory(end.tx))
    }

    fn new(stream_in: NetReader, stream_out: NetWriter) -> Self {
        Self {
            reader: RawReader {
                stream: stream_in,
                frame: Vec::new(),
                frame_length: None,
                prefix: [0; 3],
                prefix_len: 0,
                failed: false,
                compression_threshold: None,
                dec_cipher: None,
            },
            writer: RawWriter {
                stream: stream_out,
                compression_threshold: None,
                enc_cipher: None,
            },
        }
    }

    pub async fn read_packet<P: ProtocolPacket + Debug>(
        &mut self,
    ) -> Result<P, Box<ReadPacketError>> {
        let raw = self.reader.read().await?;
        deserialize_packet(&mut Cursor::new(&raw))
    }

    pub async fn write_packet<P: ProtocolPacket + Debug>(
        &mut self,
        packet: impl Packet<P>,
    ) -> io::Result<()> {
        let raw = serialize_packet(&packet.into_variant()).map_err(io::Error::other)?;
        self.writer.write(&raw).await
    }

    /// A negative threshold disables compression, per the wire format.
    pub fn set_compression_threshold(&mut self, threshold: i32) {
        let threshold = u32::try_from(threshold).ok();
        self.reader.compression_threshold = threshold;
        self.writer.compression_threshold = threshold;
    }

    /// Arms both directions. The caller must have flushed the key packet first:
    /// it is the last plaintext frame.
    pub fn set_encryption_key(&mut self, key: [u8; 16]) {
        let (enc_cipher, dec_cipher) = azalea_crypto::create_cipher(&key);
        self.reader.dec_cipher = Some(dec_cipher);
        self.writer.enc_cipher = Some(enc_cipher);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::time::Duration;

    use azalea_protocol::packets::status::ServerboundStatusPacket;
    use azalea_protocol::packets::status::s_ping_request::ServerboundPingRequest;
    use azalea_protocol::packets::status::s_status_request::ServerboundStatusRequest;
    use flate2::write::ZlibEncoder;
    use tokio::io::AsyncWriteExt as _;

    use super::*;

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    fn pipe_pair() -> (Conn, Conn) {
        let (client, server) = memory_pipes();
        (Conn::from_memory(client), Conn::from_memory(server))
    }

    fn conn_and_wire() -> (Conn, ReadHalf<SimplexStream>, WriteHalf<SimplexStream>) {
        let (client, server) = memory_pipes();
        (Conn::from_memory(client), server.rx, server.tx)
    }

    fn ping(time: u64) -> ServerboundStatusPacket {
        ServerboundStatusPacket::PingRequest(ServerboundPingRequest { time })
    }

    fn status_request() -> ServerboundStatusPacket {
        ServerboundStatusPacket::StatusRequest(ServerboundStatusRequest {})
    }

    async fn send(conn: &mut Conn, packet: ServerboundStatusPacket) {
        tokio::time::timeout(TEST_TIMEOUT, conn.write_packet(packet))
            .await
            .expect("write timed out")
            .unwrap();
    }

    async fn recv(conn: &mut Conn) -> ServerboundStatusPacket {
        tokio::time::timeout(TEST_TIMEOUT, conn.read_packet())
            .await
            .expect("read timed out")
            .unwrap()
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn compression_frame(declared: u32, compressed: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        declared.azalea_write_var(&mut frame).unwrap();
        frame.extend_from_slice(compressed);
        frame
    }

    #[test]
    fn rejects_zero_and_overwide_frame_prefixes() {
        assert!(decode_frame_length(&[0]).is_err());
        assert!(decode_frame_length(&[0x80, 0x80, 0x80]).is_err());
        assert!(decode_frame_length(&[0xff, 0xff, 0xff, 0x01]).is_err());
    }

    #[test]
    fn accepts_partial_maximum_varint21_without_allocating_body() {
        assert_eq!(
            decode_frame_length(&[0xff, 0xff, 0x7f]).unwrap(),
            Some(MAX_FRAME_LENGTH)
        );
        assert_eq!(decode_frame_length(&[0xff]).unwrap(), None);
        assert_eq!(decode_frame_length(&[0xff, 0xff]).unwrap(), None);
    }

    #[test]
    fn bounded_zlib_checks_exact_short_over_threshold_and_declared_limits() {
        let compressed = zlib(b"ok");
        assert_eq!(
            decode_compressed(&compression_frame(2, &compressed), 2)
                .unwrap()
                .as_ref(),
            b"ok"
        );
        assert!(decode_compressed(&compression_frame(3, &compressed), 2).is_err());
        assert!(decode_compressed(&compression_frame(1, &compressed), 1).is_err());
        assert!(decode_compressed(&compression_frame(2, &compressed), 3).is_err());
        assert!(decode_compressed(&compression_frame(2, &[0xff, 0x00]), 0).is_err());

        assert_eq!(
            validate_compressed_size(MAXIMUM_UNCOMPRESSED_LENGTH, 0).unwrap(),
            8 * 1024 * 1024
        );
        assert!(validate_compressed_size(MAXIMUM_UNCOMPRESSED_LENGTH + 1, 0).is_err());
    }

    #[tokio::test]
    async fn memory_pipe_round_trips_plain_and_existing_compression_settings() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            for threshold in [None, Some(-1), Some(256), Some(1)] {
                let (mut a, mut b) = pipe_pair();
                if let Some(threshold) = threshold {
                    a.set_compression_threshold(threshold);
                    b.set_compression_threshold(threshold);
                }
                send(&mut a, ping(7)).await;
                assert_eq!(recv(&mut b).await, ping(7));
            }
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn two_frames_keep_encryption_and_compression_state_in_step() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            for (encrypted, threshold) in [
                (false, None),
                (true, None),
                (false, Some(1)),
                (true, Some(1)),
            ] {
                let (mut a, mut b) = pipe_pair();
                if encrypted {
                    let key = [7u8; 16];
                    a.set_encryption_key(key);
                    b.set_encryption_key(key);
                }
                if let Some(threshold) = threshold {
                    a.set_compression_threshold(threshold);
                    b.set_compression_threshold(threshold);
                }
                send(&mut a, ping(1)).await;
                send(&mut a, ping(2)).await;
                assert_eq!(recv(&mut b).await, ping(1));
                assert_eq!(recv(&mut b).await, ping(2));
            }
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn malformed_prefix_error_makes_later_read_fail_immediately() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            for malformed_prefix in [&[0x00][..], &[0x80, 0x80, 0x80][..]] {
                let (mut conn, _wire_rx, mut peer_tx) = conn_and_wire();
                peer_tx.write_all(malformed_prefix).await.unwrap();
                assert!(conn.reader.read().await.is_err());

                let retry =
                    tokio::time::timeout(Duration::from_millis(20), conn.reader.read()).await;
                assert!(
                    matches!(retry, Ok(Err(_))),
                    "retry must fail without panic or waiting"
                );
            }
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn cancel_during_partial_prefix_then_resume() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let (mut conn, _wire_rx, mut peer_tx) = conn_and_wire();
            peer_tx.write_all(&[0x81]).await.unwrap();
            let timed = tokio::time::timeout(Duration::from_millis(20), conn.reader.read()).await;
            assert!(timed.is_err());
            peer_tx.write_all(&[0x00, 0x00, 0x01, 0x00]).await.unwrap();
            assert_eq!(conn.reader.read().await.unwrap().as_ref(), &[0x00]);
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn cancel_during_partial_body_then_resume() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let (mut conn, _wire_rx, mut peer_tx) = conn_and_wire();
            peer_tx.write_all(&[0x03, 0x00]).await.unwrap();
            let timed = tokio::time::timeout(Duration::from_millis(20), conn.reader.read()).await;
            assert!(timed.is_err());
            peer_tx.write_all(&[0x01, 0x02]).await.unwrap();
            assert_eq!(
                conn.reader.read().await.unwrap().as_ref(),
                &[0x00, 0x01, 0x02]
            );
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn frame_is_length_then_id() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let (mut conn, mut wire_rx, _peer_tx) = conn_and_wire();
            conn.write_packet(status_request()).await.unwrap();
            let mut frame = [0u8; 2];
            wire_rx.read_exact(&mut frame).await.unwrap();
            assert_eq!(frame, [0x01, 0x00]);
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test]
    async fn frames_split_across_reads_reassemble() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let (mut conn, _wire_rx, mut peer_tx) = conn_and_wire();
            for chunk in [&[0x01u8][..], &[0x00, 0x01][..], &[0x00][..]] {
                peer_tx.write_all(chunk).await.unwrap();
            }
            for _ in 0..2 {
                assert_eq!(recv(&mut conn).await, status_request());
            }
        })
        .await
        .expect("test timed out");
    }
}
