use crate::mirror::core::batching::batcher::Batcher;
use crate::mirror::core::messages::{NetworkMessageTrait, NetworkPingMessage};
use crate::mirror::core::network_messages::NetworkMessages;
use crate::mirror::core::network_time::NetworkTime;
use crate::mirror::core::network_writer_pool::NetworkWriterPool;
use crate::mirror::core::transport::{Transport, TransportChannel};
use crate::{log_error, log_warn};
use std::sync::RwLock;

pub struct NetworkConnection {
    id: u64,
    reliable_batcher: Batcher,
    unreliable_batcher: Batcher,
    is_ready: bool,
    last_message_time: f64,
    last_ping_time: f64,
    is_authenticated: bool,
    #[allow(warnings)]
    authentication_data: Option<Box<RwLock<dyn NetworkMessageTrait>>>,
    net_id: u32,
    owned: Vec<u32>,
    remote_time_stamp: f64,
    first_conn_loc_time_stamp: f64,
}

pub trait NetworkConnectionTrait {
    fn new(conn_id: u64) -> Self;
    fn net_id(&self) -> u32;
    fn set_net_id(&mut self, net_id: u32);
    fn connection_id(&self) -> u64;
    fn last_ping_time(&self) -> f64;
    fn set_last_ping_time(&mut self, time: f64);
    fn last_message_time(&self) -> f64;
    fn set_last_message_time(&mut self, time: f64);
    fn remote_time_stamp(&self) -> f64;
    fn set_remote_time_stamp(&mut self, time: f64);
    fn first_conn_loc_time_stamp(&self) -> f64;
    fn is_ready(&self) -> bool;
    fn set_ready(&mut self, ready: bool);
    fn is_authenticated(&self) -> bool;
    fn set_authenticated(&mut self, authenticated: bool);
    fn set_authenticated_data(&mut self, data: Box<RwLock<dyn NetworkMessageTrait>>);
    fn authenticated_data(&mut self) -> &Option<Box<RwLock<dyn NetworkMessageTrait>>>;
    fn owned(&mut self) -> &mut Vec<u32>;
    fn set_owned(&mut self, owned: Vec<u32>);
    fn send_network_message<T>(&mut self, message: &mut T, channel: TransportChannel)
    where
        T: NetworkMessageTrait + Send,
    {
        NetworkWriterPool::get_return(|writer| {
            message.serialize(writer);
            let max = NetworkMessages::max_message_size(channel);
            if writer.get_position() > max {
                log_error!("Message too large to send: ", writer.get_position());
                return;
            }
            self.send(writer.to_array_segment(), channel);
        });
    }
    fn send(&mut self, segment: &[u8], channel: TransportChannel);
    fn send_to_transport(&self, segment: Vec<u8>, channel: TransportChannel) {
        if let Some(transport) = Transport::active_transport() {
            transport.server_send(self.connection_id(), segment, channel);
        }
    }
    fn update(&mut self);
    fn update_ping(&mut self) {
        let local_time = NetworkTime::local_time();
        if local_time >= self.last_ping_time() + NetworkTime::get_ping_interval() {
            self.set_last_ping_time(local_time);
            self.send_network_message(
                &mut NetworkPingMessage::new(local_time, 0.0),
                TransportChannel::Unreliable,
            );
        }
    }
    fn is_alive(&self, timeout: f64) -> bool {
        let local_time = NetworkTime::local_time();
        local_time < self.last_message_time() + timeout
    }
    fn disconnect(&mut self) {
        self.set_ready(false);
    }
    fn cleanup(&mut self);
}

impl NetworkConnection {
    pub const LOCAL_CONNECTION_ID: i32 = 0;
}

impl NetworkConnectionTrait for NetworkConnection {
    fn new(conn_id: u64) -> Self {
        let ts = NetworkTime::local_time();
        let reliable_batcher_threshold = match Transport::active_transport() {
            None => {
                log_warn!("get threshold failed");
                1500
            }
            Some(active_transport) => {
                active_transport.get_batcher_threshold(TransportChannel::Reliable)
            }
        };
        let unreliable_batcher_threshold = match Transport::active_transport() {
            None => {
                log_warn!("get threshold failed");
                1500
            }
            Some(active_transport) => {
                active_transport.get_batcher_threshold(TransportChannel::Unreliable)
            }
        };
        Self {
            id: conn_id,
            is_authenticated: false,
            authentication_data: None,
            is_ready: false,
            last_message_time: ts,
            net_id: 0,
            owned: Default::default(),
            remote_time_stamp: ts,
            reliable_batcher: Batcher::new(reliable_batcher_threshold),
            unreliable_batcher: Batcher::new(unreliable_batcher_threshold),
            last_ping_time: ts,
            first_conn_loc_time_stamp: NetworkTime::local_time(),
        }
    }

    fn net_id(&self) -> u32 {
        self.net_id
    }

    fn set_net_id(&mut self, net_id: u32) {
        self.net_id = net_id;
    }

    fn connection_id(&self) -> u64 {
        self.id
    }

    fn last_ping_time(&self) -> f64 {
        self.last_ping_time
    }

    fn set_last_ping_time(&mut self, time: f64) {
        self.last_ping_time = time;
    }

    fn last_message_time(&self) -> f64 {
        self.last_message_time
    }

    fn set_last_message_time(&mut self, time: f64) {
        self.last_message_time = time;
    }

    fn remote_time_stamp(&self) -> f64 {
        self.remote_time_stamp
    }

    fn set_remote_time_stamp(&mut self, time: f64) {
        self.remote_time_stamp = time;
    }

    fn first_conn_loc_time_stamp(&self) -> f64 {
        self.first_conn_loc_time_stamp
    }

    fn is_ready(&self) -> bool {
        self.is_ready
    }

    fn set_ready(&mut self, ready: bool) {
        self.is_ready = ready;
    }

    fn is_authenticated(&self) -> bool {
        self.is_authenticated
    }

    fn set_authenticated(&mut self, authenticated: bool) {
        self.is_authenticated = authenticated;
    }

    fn set_authenticated_data(&mut self, data: Box<RwLock<dyn NetworkMessageTrait>>) {
        self.authentication_data = Some(data);
    }

    fn authenticated_data(&mut self) -> &Option<Box<RwLock<dyn NetworkMessageTrait>>> {
        &self.authentication_data
    }

    fn owned(&mut self) -> &mut Vec<u32> {
        &mut self.owned
    }

    fn set_owned(&mut self, owned: Vec<u32>) {
        self.owned = owned;
    }

    fn send(&mut self, segment: &[u8], channel: TransportChannel) {
        match channel {
            TransportChannel::Reliable => {
                self.reliable_batcher
                    .add_message(segment, NetworkTime::local_time());
            }
            TransportChannel::Unreliable => {
                self.unreliable_batcher
                    .add_message(segment, NetworkTime::local_time());
            }
        }
    }

    fn update(&mut self) {
        self.update_ping();

        NetworkWriterPool::get_return(|writer| {
            while self.reliable_batcher.get_batcher_writer(writer) {
                self.send_to_transport(writer.to_bytes(), TransportChannel::Reliable);
                writer.reset();
            }

            while self.unreliable_batcher.get_batcher_writer(writer) {
                self.send_to_transport(writer.to_bytes(), TransportChannel::Unreliable);
                writer.reset();
            }
        });
    }

    fn cleanup(&mut self) {
        self.reliable_batcher.clear();
        self.unreliable_batcher.clear();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_network_connection() {}
}
