use std::marker::PhantomData;
use std::sync::Arc;

use crate::{Address, EnetKeepAlive, Error, Event, Peer};

use enet_sys::{
    enet_host_bandwidth_limit, enet_host_channel_limit, enet_host_check_events, enet_host_connect,
    enet_host_destroy, enet_host_flush, enet_host_service, ENetEvent, ENetHost, ENetPeer,
    ENET_PROTOCOL_MAXIMUM_CHANNEL_COUNT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Represents a bandwidth limit or unlimited.
pub enum BandwidthLimit {
    /// No limit on bandwidth
    Unlimited,
    /// Bandwidth limit in bytes/second
    Limited(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Represents a channel limit or unlimited.
pub enum ChannelLimit {
    /// Maximum limit on the number of channels
    Maximum,
    /// Channel limit
    Limited(usize),
}

impl ChannelLimit {
    pub(in crate) fn to_enet_usize(&self) -> usize {
        match *self {
            ChannelLimit::Maximum => 0,
            ChannelLimit::Limited(l) => l,
        }
    }

    fn from_enet_usize(enet_val: usize) -> ChannelLimit {
        const MAX_COUNT: usize = ENET_PROTOCOL_MAXIMUM_CHANNEL_COUNT as usize;
        match enet_val {
            MAX_COUNT => ChannelLimit::Maximum,
            0 => panic!("ChannelLimit::from_enet_usize: got 0"),
            lim => ChannelLimit::Limited(lim),
        }
    }
}

impl BandwidthLimit {
    pub(in crate) fn to_enet_u32(&self) -> u32 {
        match *self {
            BandwidthLimit::Unlimited => 0,
            BandwidthLimit::Limited(l) => l,
        }
    }

    /// Create a bandwidth limit value from a u32. 0 is unlimited
    pub fn from_enet_u32(enet_val: u32) -> BandwidthLimit {
        match enet_val {
            0 => BandwidthLimit::Unlimited,
            l => BandwidthLimit::Limited(l),
        }
    }
}

/// A `Host` represents one endpoint of an ENet connection. Created through `Enet`.
///
/// This type provides functionality such as connection establishment and packet transmission.
pub struct Host<T> {
    inner: *mut ENetHost,

    _keep_alive: Arc<EnetKeepAlive>,
    _peer_data: PhantomData<*const T>,
}

impl<T> Host<T> {
    pub(in crate) fn new(_keep_alive: Arc<EnetKeepAlive>, inner: *mut ENetHost) -> Host<T> {
        assert!(!inner.is_null());

        Host {
            inner,
            _keep_alive,
            _peer_data: PhantomData,
        }
    }

    /// Sends any queued packets on the host specified to its designated peers.
    ///
    /// This function need only be used in circumstances where one wishes to send queued packets earlier than in a call to `Host::service()`.
    pub fn flush(&mut self) {
        unsafe {
            enet_host_flush(self.inner);
        }
    }

    /// Sets the bandwith limits for this `Host`.
    pub fn set_bandwith_limits(
        &mut self,
        incoming_bandwith: BandwidthLimit,
        outgoing_bandwidth: BandwidthLimit,
    ) {
        unsafe {
            enet_host_bandwidth_limit(
                self.inner,
                incoming_bandwith.to_enet_u32(),
                outgoing_bandwidth.to_enet_u32(),
            );
        }
    }

    /// Sets the maximum allowed channels of future connections.
    pub fn set_channel_limit(&mut self, max_channel_count: ChannelLimit) {
        unsafe {
            enet_host_channel_limit(self.inner, max_channel_count.to_enet_usize());
        }
    }

    /// Returns the limit of channels per connected peer for this `Host`.
    pub fn channel_limit(&self) -> ChannelLimit {
        ChannelLimit::from_enet_usize(unsafe { (*self.inner).channelLimit })
    }

    /// Returns the downstream bandwidth of this `Host` in bytes/second.
    pub fn incoming_bandwidth(&self) -> u32 {
        unsafe { (*self.inner).incomingBandwidth }
    }

    /// Returns the upstream bandwidth of this `Host` in bytes/second.
    pub fn outgoing_bandwidth(&self) -> u32 {
        unsafe { (*self.inner).outgoingBandwidth }
    }

    /// Returns the internet address of this `Host`.
    pub fn address(&self) -> Address {
        Address::from_enet_address(&unsafe { (*self.inner).address })
    }

    /// Returns the number of peers allocated for this `Host`.
    pub fn peer_count(&self) -> usize {
        unsafe { (*self.inner).peerCount }
    }

    /// Returns an iterator over all peers connected to this `Host`.
    pub fn get_peer(&'_ self, index: usize) -> Peer<'_, T> {
        let raw_peers =
            unsafe { std::slice::from_raw_parts_mut((*self.inner).peers, (*self.inner).peerCount) };

        Peer::new(&mut raw_peers[index])
    }

    /// Returns an iterator over all peers connected to this `Host`.
    pub fn peers(&'_ self) -> impl Iterator<Item = Peer<'_, T>> {
        let raw_peers =
            unsafe { std::slice::from_raw_parts_mut((*self.inner).peers, (*self.inner).peerCount) };

        raw_peers.into_iter().map(|rp| Peer::new(rp))
    }

    /// Returns an iterator over all peers connected to this `Host`.
    pub fn peers_mut(&'_ mut self) -> impl Iterator<Item = Peer<'_, T>> {
        let raw_peers =
            unsafe { std::slice::from_raw_parts_mut((*self.inner).peers, (*self.inner).peerCount) };

        raw_peers.into_iter().map(|rp| Peer::new(rp))
    }

    /// Maintains this host and delivers an event if available.
    ///
    /// This should be called regularly for ENet to work properly with good performance.
    pub fn service(&'_ mut self, timeout_ms: u32) -> Result<Option<Event<'_, T>>, Error> {
        // ENetEvent is Copy (aka has no Drop impl), so we don't have to make sure we `mem::forget` it later on
        let mut sys_event: ENetEvent = unsafe { std::mem::uninitialized() };

        let res =
            unsafe { enet_host_service(self.inner, &mut sys_event as *mut ENetEvent, timeout_ms) };

        match res {
            r if r > 0 => Ok(Event::from_sys_event(self, &sys_event)),
            0 => Ok(None),
            r if r < 0 => Err(Error(r)),
            _ => panic!("unreachable"),
        }

        // TODO: check `total*` fields on `inner`, these need to be reset from time to time.
    }

    /// Checks for any queued events on this `Host` and dispatches one if available
    pub fn check_events(&'_ mut self) -> Result<Option<Event<'_, T>>, Error> {
        // ENetEvent is Copy (aka has no Drop impl), so we don't have to make sure we `mem::forget` it later on
        let mut sys_event: ENetEvent = unsafe { std::mem::uninitialized() };

        let res = unsafe { enet_host_check_events(self.inner, &mut sys_event as *mut ENetEvent) };

        match res {
            r if r > 0 => Ok(Event::from_sys_event(self, &sys_event)),
            0 => Ok(None),
            r if r < 0 => Err(Error(r)),
            _ => panic!("unreachable"),
        }
    }

    /// Initiates a connection to a foreign host.
    ///
    /// The connection will not be done until a `Event::Connected` for this peer was received.
    ///
    /// `channel_count` specifies how many channels to allocate for this peer.
    /// `user_data` is a user-specified value that can be chosen arbitrarily.
    pub fn connect(
        &mut self,
        address: &Address,
        channel_count: usize,
        user_data: u32,
    ) -> Result<Peer<'_, T>, Error> {
        let res: *mut ENetPeer = unsafe {
            enet_host_connect(
                self.inner,
                &address.to_enet_address() as *const _,
                channel_count,
                user_data,
            )
        };

        if res.is_null() {
            return Err(Error(0));
        }

        Ok(Peer::new(res))
    }
}

impl<T> Drop for Host<T> {
    /// Call the corresponding ENet cleanup-function(s).
    fn drop(&mut self) {
        unsafe {
            enet_host_destroy(self.inner);
        }
    }
}
