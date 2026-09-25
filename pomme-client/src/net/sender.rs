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
}

/// A `LastSeenMessagesTracker` update, recorded by the chat UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatMark {
    Processed { signature: [u8; 256], shown: bool },
    Deleted { signature: [u8; 256] },
}

pub struct PacketSender {
    tx: mpsc::UnboundedSender<Outbound>,
}

fn encode_place_recipe(packet_id: u32, container_id: i32, display_id: u32) -> Vec<u8> {
    use pomme_protocol::wire;

    let mut frame = Vec::with_capacity(13);
    wire::write_varint(&mut frame, packet_id);
    wire::write_varint(&mut frame, container_id as u32);
    wire::write_varint(&mut frame, display_id);
    frame.push(0); // use_max_items=false; the UI has no modifier variant yet.
    frame
}

impl PacketSender {
    pub fn new(tx: mpsc::UnboundedSender<Outbound>) -> Self {
        Self { tx }
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

    /// Sends native 26.2 PlaceRecipe, whose recipe reference is a numeric
    /// server-issued display ID (Azalea's typed packet uses the legacy
    /// Identifier).
    pub fn place_recipe(&self, container_id: i32, display_id: u32) {
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
        self.send_raw(encode_place_recipe(packet_id, container_id, display_id));
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
        if let Err(e) = self.tx.send(out) {
            tracing::error!("Failed to queue outbound packet: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use azalea_protocol::packets::game::ServerboundGamePacket;
    use tokio::sync::mpsc;

    use super::{Outbound, PacketSender};

    #[test]
    fn place_recipe_wire_body_includes_display_id_and_use_max_items_flag() {
        let frame = super::encode_place_recipe(0x2a, 3, 300);
        assert_eq!(frame, [0x2a, 3, 0xac, 0x02, 0]);
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
