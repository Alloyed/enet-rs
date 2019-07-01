use enet_sys::{
    enet_packet_create, enet_packet_destroy, ENetPacket, _ENetPacketFlag_ENET_PACKET_FLAG_RELIABLE,
    _ENetPacketFlag_ENET_PACKET_FLAG_UNSEQUENCED,
};

use crate::Error;

/// A packet that can be sent or retrieved on an ENet-connection.
#[derive(Debug)]
pub struct Packet {
    inner: *mut ENetPacket,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
/// Mode that can be set when transmitting a packet.
///
/// ENet does not support reliable but unsequenced packets.
pub enum PacketMode {
    /// The packet will be sent unreliably but sequenced (ENet default).
    UnreliableSequenced,
    /// The packet will be sent unreliably and unsequenced.
    UnreliableUnsequenced,
    /// The packet will be sent reliably and sequenced with other reliable packets.
    ReliableSequenced,
}

impl PacketMode {
    /// Returns whether this represents a reliable mode.
    pub fn is_reliable(&self) -> bool {
        match self {
            PacketMode::UnreliableSequenced => false,
            PacketMode::UnreliableUnsequenced => false,
            PacketMode::ReliableSequenced => true,
        }
    }

    /// Returns whether this represents a sequenced mode.
    pub fn is_sequenced(&self) -> bool {
        match self {
            PacketMode::UnreliableSequenced => true,
            PacketMode::UnreliableUnsequenced => false,
            PacketMode::ReliableSequenced => true,
        }
    }

    const UNSEQUENCED_FLAG: u32 = _ENetPacketFlag_ENET_PACKET_FLAG_UNSEQUENCED as u32;
    const RELIABLE_FLAG: u32 = _ENetPacketFlag_ENET_PACKET_FLAG_RELIABLE as u32;
    fn to_sys_flags(&self) -> u32 {
        match self {
            PacketMode::UnreliableSequenced => 0,
            PacketMode::UnreliableUnsequenced => PacketMode::UNSEQUENCED_FLAG,
            PacketMode::ReliableSequenced => PacketMode::RELIABLE_FLAG,
        }
    }

    fn from_sys_flags(sys_flags: u32) -> PacketMode {
        match sys_flags {
            0 => PacketMode::UnreliableSequenced,
            PacketMode::UNSEQUENCED_FLAG => PacketMode::UnreliableUnsequenced,
            PacketMode::RELIABLE_FLAG => PacketMode::ReliableSequenced,
            _ => panic!("Invalid sysflag"),
        }
    }

    /// Returns a packet mode from matching against an input string.
    pub fn from_string(string: &str) -> Option<PacketMode> {
        match string {
            "unreliable" => Some(PacketMode::UnreliableSequenced),
            "unsequenced" => Some(PacketMode::UnreliableUnsequenced),
            "reliable" => Some(PacketMode::ReliableSequenced),
            _ => None,
        }
    }
}

impl Packet {
    /// Creates a new Packet with optional reliability settings.
    pub fn new(data: &[u8], mode: PacketMode) -> Result<Packet, Error> {
        let res = unsafe {
            enet_packet_create(data.as_ptr() as *const _, data.len(), mode.to_sys_flags())
        };

        if res.is_null() {
            return Err(Error(0));
        }

        Ok(Packet::from_sys_packet(res))
    }

    // TODO: this should be a clone
    /// Returns a copy of this packet
    pub fn copy(other_packet: &Packet) -> Result<Packet, Error> {
        Packet::new(other_packet.data(), other_packet.packet_mode())
    }

    pub(crate) fn from_sys_packet(inner: *mut ENetPacket) -> Packet {
        Packet { inner }
    }

    /// Does NOT run this `Packet`'s destructor.
    pub(crate) fn into_inner(self) -> *mut ENetPacket {
        let res = self.inner;
        std::mem::forget(self);
        res
    }

    /// Returns a reference to the bytes inside this packet.
    pub fn data<'a>(&'a self) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts((*self.inner).data, (*self.inner).dataLength) }
    }

    /// Returns the delivery mechanism for this packet.
    pub fn packet_mode<'a>(&'a self) -> PacketMode {
        unsafe { PacketMode::from_sys_flags((*self.inner).flags) }
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe {
            enet_packet_destroy(self.inner);
        }
    }
}
