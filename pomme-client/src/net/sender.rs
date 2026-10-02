use azalea_protocol::packets::game::ServerboundGamePacket;
use tokio::sync::mpsc;

/// An outbound game packet: either an azalea-serialized packet or bytes
/// pre-encoded by `net::wire` (varint packet id + body), or chat work the
/// network loop signs and tracks. Chat shares the queue so it applies in
/// game-thread order, like vanilla's single-threaded `ClientPacketListener`.
pub enum Outbound {
    Packet(Box<ServerboundGamePacket>),
    Raw(Vec<u8>),
    /// Typed chat, or a command with its leading `/`.
    ChatInput(String),
    /// Vanilla `handleLogin`'s chat reset.
    ChatLogin {
        online_mode: bool,
    },
    ChatMark(Box<ChatMark>),
    /// A dialog or chat `custom` click, encoded for whichever phase the
    /// connection is in when it goes out.
    CustomClick {
        id: String,
        payload: Option<simdnbt::owned::NbtTag>,
    },
    CodeOfConductDecision(bool),
    /// Recording-session correlation envelope; removed by the connection loop
    /// before encoding.
    Traced {
        trace: crate::movement_record::PacketTraceId,
        packet: Box<Outbound>,
    },
}

/// A `LastSeenMessagesTracker` update, recorded by the chat UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatMark {
    Processed { signature: [u8; 256], shown: bool },
    Deleted { signature: [u8; 256] },
}

pub struct PacketSender {
    tx: mpsc::UnboundedSender<Outbound>,
    pub recorder: std::sync::Arc<crate::movement_record::Recorder>,
}

fn encode_sign_update(
    packet_id: u32,
    pos: azalea_core::position::BlockPos,
    front: bool,
    lines: &[String; 4],
) -> Vec<u8> {
    use pomme_protocol::wire;
    let packed = (((pos.x as i64 & 0x3ff_ffff) << 38)
        | ((pos.z as i64 & 0x3ff_ffff) << 12)
        | (pos.y as i64 & 0xfff)) as u64;
    let mut frame = Vec::new();
    wire::write_varint(&mut frame, packet_id);
    frame.extend_from_slice(&packed.to_be_bytes());
    frame.push(u8::from(front));
    for line in lines {
        wire::write_varint(&mut frame, line.len() as u32);
        frame.extend_from_slice(line.as_bytes());
    }
    frame
}

fn encode_set_command_block(
    packet_id: u32,
    pos: azalea_core::position::BlockPos,
    command: &str,
    mode: u32,
    flags: u8,
) -> Option<Vec<u8>> {
    if mode > 2 || command.encode_utf16().count() > 32_767 || command.len() > 131_068 {
        return None;
    }
    use pomme_protocol::wire;
    let packed = (((pos.x as i64 & 0x3ff_ffff) << 38)
        | ((pos.z as i64 & 0x3ff_ffff) << 12)
        | (pos.y as i64 & 0xfff)) as u64;
    let mut frame = Vec::new();
    wire::write_varint(&mut frame, packet_id);
    frame.extend_from_slice(&packed.to_be_bytes());
    wire::write_varint(&mut frame, command.len() as u32);
    frame.extend_from_slice(command.as_bytes());
    wire::write_varint(&mut frame, mode);
    frame.push(flags);
    Some(frame)
}

fn encode_recipe_book_settings(
    packet_id: u32,
    book_type: u32,
    open: bool,
    filtering: bool,
) -> Vec<u8> {
    use pomme_protocol::wire;

    let mut frame = Vec::with_capacity(4);
    wire::write_varint(&mut frame, packet_id);
    wire::write_varint(&mut frame, book_type);
    frame.push(u8::from(open));
    frame.push(u8::from(filtering));
    frame
}

fn encode_place_recipe(
    packet_id: u32,
    container_id: i32,
    display_id: u32,
    use_max_items: bool,
) -> Vec<u8> {
    use pomme_protocol::wire;

    let mut frame = Vec::with_capacity(13);
    wire::write_varint(&mut frame, packet_id);
    wire::write_varint(&mut frame, container_id as u32);
    wire::write_varint(&mut frame, display_id);
    frame.push(u8::from(use_max_items));
    frame
}

impl PacketSender {
    pub fn new(tx: mpsc::UnboundedSender<Outbound>) -> Self {
        Self::with_recorder(tx, Default::default())
    }

    pub fn with_recorder(
        tx: mpsc::UnboundedSender<Outbound>,
        recorder: std::sync::Arc<crate::movement_record::Recorder>,
    ) -> Self {
        Self { tx, recorder }
    }

    pub fn send(&self, packet: ServerboundGamePacket) {
        self.queue(Outbound::Packet(Box::new(packet)));
    }

    /// Select a merchant offer using the dedicated 26.2 ServerboundSelectTrade
    /// packet (not ServerboundContainerButtonClick).
    pub fn select_trade(&self, index: u32) {
        self.send(ServerboundGamePacket::SelectTrade(
            azalea_protocol::packets::game::s_select_trade::ServerboundSelectTrade { item: index },
        ));
    }

    /// Select the primary/secondary beacon effects using the dedicated 26.2
    /// `set_beacon` packet, not a container button click.
    pub fn set_beacon(&self, primary: Option<u32>, secondary: Option<u32>) {
        self.send(ServerboundGamePacket::SetBeacon(
            azalea_protocol::packets::game::s_set_beacon::ServerboundSetBeacon {
                primary,
                secondary,
            },
        ));
    }

    /// EditBook has the same bounded layout in native 26.2 and Azalea.
    pub fn edit_book(&self, slot: u32, pages: Vec<String>, title: Option<String>) -> bool {
        if !matches!(slot, 0..=8 | 40)
            || pages.len() > 100
            || pages.iter().any(|page| page.encode_utf16().count() > 1024)
            || title
                .as_ref()
                .is_some_and(|title| title.encode_utf16().count() > 32)
        {
            tracing::warn!("Rejected outbound book exceeding protocol limits");
            return false;
        }
        self.send(ServerboundGamePacket::EditBook(
            azalea_protocol::packets::game::s_edit_book::ServerboundEditBook { slot, pages, title },
        ));
        true
    }

    /// Sends the native recipe-book type and its open/filter settings.
    pub fn recipe_book_settings(&self, book_type: u32, open: bool, filtering: bool) {
        if crate::version::session_protocol() != pomme_protocol::version::NATIVE.protocol {
            return;
        }
        use pomme_protocol::{Direction, PacketTable, Phase};
        let Some(packet_id) = PacketTable::for_protocol(crate::version::session_protocol())
            .and_then(|table| {
                table.id(
                    Phase::Game,
                    Direction::Serverbound,
                    "recipe_book_change_settings",
                )
            })
        else {
            tracing::warn!("Native 26.2 recipe-book settings packet ID is unavailable");
            return;
        };
        self.send_raw(encode_recipe_book_settings(
            packet_id, book_type, open, filtering,
        ));
    }

    /// Sends native 26.2 PlaceRecipe, whose recipe reference is a numeric
    /// server-issued display ID (Azalea's typed packet uses the legacy
    /// Identifier).
    pub fn place_recipe(&self, container_id: i32, display_id: u32, use_max_items: bool) {
        if crate::version::session_protocol() != pomme_protocol::version::NATIVE.protocol {
            return;
        }
        use pomme_protocol::{Direction, PacketTable, Phase};
        let Some(packet_id) = PacketTable::for_protocol(crate::version::session_protocol())
            .and_then(|table| table.id(Phase::Game, Direction::Serverbound, "place_recipe"))
        else {
            tracing::warn!("Native 26.2 PlaceRecipe packet ID is unavailable");
            return;
        };
        self.send_raw(encode_place_recipe(
            packet_id,
            container_id,
            display_id,
            use_max_items,
        ));
    }

    /// Sends the pinned native 26.2 SignUpdate packet; no client-side
    /// authorization is attempted (the server checks wax and edit UUID).
    pub fn sign_update(
        &self,
        pos: azalea_core::position::BlockPos,
        front: bool,
        lines: [String; 4],
    ) {
        if crate::version::session_protocol() != pomme_protocol::version::NATIVE.protocol {
            return;
        }
        use pomme_protocol::{Direction, PacketTable, Phase};
        let Some(id) = PacketTable::for_protocol(crate::version::session_protocol())
            .and_then(|table| table.id(Phase::Game, Direction::Serverbound, "sign_update"))
        else {
            tracing::warn!("Native 26.2 SignUpdate packet ID is unavailable");
            return;
        };
        self.send_raw(encode_sign_update(id, pos, front, &lines));
    }

    /// Sends the native 26.2 command-block edit packet. Older negotiated
    /// versions are rejected: their raw payload layouts are not translated.
    pub fn set_command_block(
        &self,
        pos: azalea_core::position::BlockPos,
        command: &str,
        mode: u32,
        flags: u8,
    ) -> bool {
        if crate::version::session_protocol() != pomme_protocol::version::NATIVE.protocol {
            return false;
        }
        use pomme_protocol::{Direction, PacketTable, Phase};
        let Some(id) =
            PacketTable::native().id(Phase::Game, Direction::Serverbound, "set_command_block")
        else {
            return false;
        };
        let Some(frame) = encode_set_command_block(id, pos, command, mode, flags) else {
            return false;
        };
        self.send_raw(frame);
        true
    }

    pub fn send_raw(&self, bytes: Vec<u8>) {
        self.queue(Outbound::Raw(bytes));
    }

    pub fn send_chat(&self, input: String) {
        self.queue(Outbound::ChatInput(input));
    }

    pub fn chat_login(&self, online_mode: bool) {
        self.queue(Outbound::ChatLogin { online_mode });
    }

    pub fn mark_chat(&self, mark: ChatMark) {
        self.queue(Outbound::ChatMark(Box::new(mark)));
    }

    pub fn send_custom_click(&self, id: String, payload: Option<simdnbt::owned::NbtTag>) {
        self.queue(Outbound::CustomClick { id, payload });
    }

    pub fn decide_code_of_conduct(&self, accept: bool) {
        self.queue(Outbound::CodeOfConductDecision(accept));
    }

    fn queue(&self, out: Outbound) {
        let trace = self.recorder.packet_trace_id();
        let observation = if self.recorder.active() {
            match &out {
                Outbound::Packet(p) => crate::movement_record::outbound(p),
                Outbound::Raw(frame) => crate::movement_record::outbound_frame(frame),
                _ => None,
            }
        } else {
            None
        };
        if let Some(trace) = trace {
            self.recorder.packet_stage(trace, "queue_attempt", None);
        } else {
            self.recorder
                .record("outbound", "queue_attempt", || observation.clone());
        }
        let out = match trace {
            Some(trace) => Outbound::Traced {
                trace,
                packet: Box::new(out),
            },
            None => out,
        };
        match self.tx.send(out) {
            Ok(()) => {
                if let Some(trace) = trace {
                    self.recorder.packet_stage(trace, "queued", None);
                } else {
                    self.recorder.record("outbound", "queued", || observation);
                }
            }
            Err(e) => {
                if let Some(trace) = trace {
                    self.recorder
                        .packet_stage(trace, "queue_failed", Some("channel_closed"));
                } else {
                    self.recorder
                        .record("outbound", "queue_failed", || observation);
                }
                tracing::error!("Failed to queue outbound packet: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use azalea_protocol::packets::game::ServerboundGamePacket;
    use tokio::sync::mpsc;

    use super::{Outbound, PacketSender, encode_set_command_block};

    #[test]
    fn edit_book_validates_before_queueing_and_keeps_typed_wire_layout() {
        use std::io::Cursor;

        use azalea_buf::AzBuf;
        use azalea_protocol::packets::game::s_edit_book::ServerboundEditBook;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        for (slot, pages, title) in [
            (9, vec![], None),
            (41, vec![], None),
            (0, vec![String::new(); 101], None),
            (0, vec!["😀".repeat(513)], None),
            (40, vec![], Some("😀".repeat(17))),
        ] {
            assert!(!sender.edit_book(slot, pages, title));
            assert!(rx.try_recv().is_err());
        }
        for slot in [0, 8, 40] {
            assert!(sender.edit_book(slot, vec!["page".into()], Some("Title".into())));
            let Outbound::Packet(packet) = rx.try_recv().unwrap() else {
                panic!("typed book packet");
            };
            let ServerboundGamePacket::EditBook(packet) = *packet else {
                panic!("EditBook");
            };
            let mut body = Vec::new();
            packet.azalea_write(&mut body).unwrap();
            assert_eq!(
                body,
                [
                    vec![slot as u8, 1, 4],
                    b"page".to_vec(),
                    vec![1, 5],
                    b"Title".to_vec()
                ]
                .concat()
            );
            let decoded =
                ServerboundEditBook::azalea_read(&mut Cursor::new(body.as_slice())).unwrap();
            assert_eq!(decoded.slot, slot);
            assert_eq!(decoded.pages, ["page"]);
            assert_eq!(decoded.title.as_deref(), Some("Title"));
        }
    }

    #[test]
    fn sign_update_encodes_position_face_and_four_strings_in_native_order() {
        let pos = azalea_core::position::BlockPos { x: -2, y: 63, z: 9 };
        let bytes = super::encode_sign_update(
            7,
            pos,
            false,
            &["a".into(), "b".into(), "".into(), "d".into()],
        );
        let packed = (((-2_i64 & 0x3ff_ffff) << 38) | ((9_i64 & 0x3ff_ffff) << 12) | 63) as u64;
        assert_eq!(
            &bytes[..9],
            &[
                7,
                (packed >> 56) as u8,
                (packed >> 48) as u8,
                (packed >> 40) as u8,
                (packed >> 32) as u8,
                (packed >> 24) as u8,
                (packed >> 16) as u8,
                (packed >> 8) as u8,
                packed as u8
            ]
        );
        assert_eq!(&bytes[9..], &[0, 1, b'a', 1, b'b', 0, 1, b'd']);
    }

    #[test]
    fn recipe_book_settings_wire_order_matches_native_26_2_packet() {
        // protocol-26.2.json game.serverbound index 46 => packet ID 0x2e.
        for (book_type, encoded_type) in [(0, 0), (1, 1), (2, 2), (3, 3)] {
            assert_eq!(
                super::encode_recipe_book_settings(0x2e, book_type, true, false),
                [0x2e, encoded_type, 1, 0]
            );
        }
    }

    #[test]
    fn place_recipe_wire_body_includes_display_id_and_use_max_items_flag() {
        let normal = super::encode_place_recipe(0x2a, 3, 300, false);
        assert_eq!(normal, [0x2a, 3, 0xac, 0x02, 0]);
        let shift = super::encode_place_recipe(0x2a, 3, 300, true);
        assert_eq!(shift, [0x2a, 3, 0xac, 0x02, 1]);
    }

    #[test]
    fn select_trade_queues_the_dedicated_trade_index_packet() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        PacketSender::new(tx).select_trade(6);
        let Outbound::Packet(packet) = rx.try_recv().unwrap() else {
            panic!("trade selection must be a game packet");
        };
        let ServerboundGamePacket::SelectTrade(packet) = *packet else {
            panic!("trade selection must not use ContainerButtonClick");
        };
        assert_eq!(packet.item, 6);
    }

    #[test]
    fn command_block_frame_encodes_signed_position_unicode_mode_and_flags() {
        let pos = azalea_core::position::BlockPos::new(-1, -2, -3);
        let frame = encode_set_command_block(7, pos, "say 🌙", 1, 5).unwrap();
        let mut cursor = 0;
        assert_eq!(
            pomme_protocol::wire::read_varint(&frame, &mut cursor),
            Some(7)
        );
        let packed = u64::from_be_bytes(frame[cursor..cursor + 8].try_into().unwrap());
        assert_eq!((packed >> 38) & 0x3ff_ffff, 0x3ff_ffff);
        assert_eq!((packed >> 12) & 0x3ff_ffff, 0x3ff_fffd);
        assert_eq!(packed & 0xfff, 0xffe);
        cursor += 8;
        let len = pomme_protocol::wire::read_varint(&frame, &mut cursor).unwrap() as usize;
        assert_eq!(&frame[cursor..cursor + len], "say 🌙".as_bytes());
        cursor += len;
        assert_eq!(
            pomme_protocol::wire::read_varint(&frame, &mut cursor),
            Some(1)
        );
        assert_eq!(frame[cursor], 5);
        assert!(encode_set_command_block(7, pos, "x", 3, 0).is_none());
        assert!(encode_set_command_block(7, pos, &"x".repeat(32_768), 0, 0).is_none());
    }

    #[test]
    fn set_beacon_queues_primary_and_secondary_effect_ids() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        PacketSender::new(tx).set_beacon(Some(1), Some(9));
        let Outbound::Packet(packet) = rx.try_recv().unwrap() else {
            panic!("beacon selection must be a game packet");
        };
        let ServerboundGamePacket::SetBeacon(packet) = *packet else {
            panic!("beacon selection must not use ContainerButtonClick");
        };
        assert_eq!(packet.primary, Some(1));
        assert_eq!(packet.secondary, Some(9));
    }
}
