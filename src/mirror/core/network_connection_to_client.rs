use crate::mirror::core::network_connection::{NetworkConnection, NetworkConnectionTrait};
use crate::mirror::core::network_identity::NetworkIdentity;
use crate::mirror::core::network_manager::NetworkManagerStatic;
use crate::mirror::core::network_server::{NetworkServer, NetworkServerStatic, RemovePlayerOptions};
use crate::mirror::core::network_time::{ExponentialMovingAverage, NetworkTime};
use crate::mirror::core::network_writer::NetworkWriter;
use crate::mirror::core::snapshot_interpolation::snapshot_interpolation::SnapshotInterpolation;
use crate::mirror::core::snapshot_interpolation::time_snapshot::TimeSnapshot;
use crate::mirror::core::transport::{Transport, TransportChannel};
use ordered_float::OrderedFloat;
use std::collections::BTreeMap;

pub struct NetworkConnectionToClient {
    network_connection: NetworkConnection,
    pub reliable_rpcs_batch: NetworkWriter,
    pub unreliable_rpcs_batch: NetworkWriter,
    pub address: String,
    pub observing: Vec<u32>,
    pub drift_ema: ExponentialMovingAverage,
    pub delivery_time_ema: ExponentialMovingAverage,
    pub remote_timeline: f64,
    pub remote_timescale: f64,
    pub buffer_time_multiplier: f64,
    pub buffer_time: f64,
    pub snapshots: BTreeMap<OrderedFloat<f64>, TimeSnapshot>,
    pub snapshot_buffer_size_limit: i32,
    pub _rtt: ExponentialMovingAverage,
}
impl NetworkConnectionTrait for NetworkConnectionToClient {
    fn new(conn_id: u64) -> Self {
        let ts = NetworkTime::local_time();
        let mut network_connection_to_client = Self {
            network_connection: NetworkConnection::new(conn_id),
            reliable_rpcs_batch: NetworkWriter::new(),
            unreliable_rpcs_batch: NetworkWriter::new(),
            address: "".to_string(),
            observing: Vec::new(),
            drift_ema: ExponentialMovingAverage::new(60),
            delivery_time_ema: ExponentialMovingAverage::new(10),
            remote_timeline: ts,
            remote_timescale: ts,
            buffer_time_multiplier: 2.0,
            buffer_time: 0.0,
            snapshots: Default::default(),
            snapshot_buffer_size_limit: 64,
            _rtt: ExponentialMovingAverage::new(NetworkTime::PING_WINDOW_SIZE),
        };
        network_connection_to_client.buffer_time = NetworkServerStatic::get_static_send_interval() as f64 * network_connection_to_client.buffer_time_multiplier;
        if let Some(mut transport) = Transport::get_active_transport() {
            network_connection_to_client.address = transport.server_get_client_address(conn_id);
        }
        network_connection_to_client
    }

    fn net_id(&self) -> u32 {
        self.network_connection.net_id()
    }

    fn set_net_id(&mut self, net_id: u32) {
        self.network_connection.set_net_id(net_id);
    }

    fn connection_id(&self) -> u64 {
        self.network_connection.connection_id()
    }

    fn last_ping_time(&self) -> f64 {
        self.network_connection.last_ping_time()
    }

    fn set_last_ping_time(&mut self, time: f64) {
        self.network_connection.set_last_ping_time(time);
    }

    fn last_message_time(&self) -> f64 {
        self.network_connection.last_message_time()
    }

    fn set_last_message_time(&mut self, time: f64) {
        self.network_connection.set_last_message_time(time);
    }

    fn remote_time_stamp(&self) -> f64 {
        self.network_connection.remote_time_stamp()
    }

    fn set_remote_time_stamp(&mut self, time: f64) {
        self.network_connection.set_remote_time_stamp(time);
    }

    fn is_ready(&self) -> bool {
        self.network_connection.is_ready()
    }

    fn set_ready(&mut self, ready: bool) {
        self.network_connection.set_ready(ready);
    }

    fn is_authenticated(&self) -> bool {
        self.network_connection.is_authenticated()
    }

    fn set_authenticated(&mut self, authenticated: bool) {
        self.network_connection.set_authenticated(authenticated);
    }

    fn owned(&mut self) -> &mut Vec<u32> {
        self.network_connection.owned()
    }

    fn set_owned(&mut self, owned: Vec<u32>) {
        self.network_connection.set_owned(owned);
    }

    fn send(&mut self, segment: &[u8], channel: TransportChannel) {
        self.network_connection.send(segment, channel);
    }

    fn update(&mut self) {
        self.network_connection.update();
    }

    fn disconnect(&mut self) {
        self.reliable_rpcs_batch.reset();
        self.unreliable_rpcs_batch.reset();
        self.network_connection.disconnect();
    }

    fn cleanup(&mut self) {
        self.network_connection.cleanup();
    }
}

impl NetworkConnectionToClient {
    pub fn on_time_snapshot(&mut self, snapshot: TimeSnapshot) {
        if self.snapshots.len() >= self.snapshot_buffer_size_limit as usize {
            return;
        }

        let snapshot_settings = NetworkManagerStatic::get_network_manager_singleton().snapshot_interpolation_settings();

        // dynamic adjustment
        if snapshot_settings.dynamic_adjustment {
            self.buffer_time_multiplier = SnapshotInterpolation::dynamic_adjustment(
                NetworkServerStatic::get_static_send_interval() as f64,
                self.delivery_time_ema.standard_deviation,
                snapshot_settings.dynamic_adjustment_tolerance as f64,
            )
        }

        SnapshotInterpolation::insert_and_adjust(
            &mut self.snapshots,
            self.snapshot_buffer_size_limit as usize,
            snapshot,
            &mut self.remote_timeline,
            &mut self.remote_timescale,
            NetworkServerStatic::get_static_send_interval() as f64,
            self.buffer_time,
            snapshot_settings.catchup_speed,
            snapshot_settings.slowdown_speed,
            &mut self.drift_ema,
            snapshot_settings.catchup_negative_threshold as f64,
            snapshot_settings.catchup_positive_threshold as f64,
            &mut self.delivery_time_ema,
        );
    }
    pub fn update_time_interpolation(&mut self) {
        if self.snapshots.len() > 0 {
            SnapshotInterpolation::step_time(
                NetworkTime::get_ping_interval(),
                &mut self.remote_timeline,
                self.remote_timescale,
            );

            SnapshotInterpolation::step_interpolation(
                &mut self.snapshots,
                self.remote_timeline,
            );
        }
    }
    pub fn add_to_observing(&mut self, network_identity: &mut NetworkIdentity) {
        self.observing.push(network_identity.net_id());
        NetworkServer::show_for_connection(network_identity, self);
    }
    pub fn remove_from_observing_identities(&mut self, network_identity: &mut NetworkIdentity, is_destroyed: bool) {
        self.observing.retain(|net_id| *net_id != network_identity.net_id());
        if !is_destroyed {
            NetworkServer::hide_for_connection(network_identity, self);
        }
    }
    // void RemoveFromObservingsObservers()
    pub fn remove_from_observings_observers(&mut self) {
        let conn_id = self.connection_id();
        for net_id in self.observing.iter_mut() {
            if let Some(mut identity) = NetworkServerStatic::get_static_spawned_network_identities().get_mut(net_id) {
                identity.remove_observer(conn_id);
            }
        }
        self.observing.clear();
    }

    pub fn add_owned_object(&mut self, net_id: u32) {
        self.owned().push(net_id);
    }
    pub fn remove_owned_object(&mut self, net_id: u32) {
        self.owned().retain(|x| net_id != net_id);
    }

    pub fn destroy_owned_objects(&mut self) {
        for i in 0..self.owned().len() {
            let net_id = self.owned()[i];
            if net_id != 0 {
                if let Some(mut identity) = NetworkServerStatic::get_static_spawned_network_identities().get_mut(&net_id) {
                    if identity.scene_id != 0 {
                        NetworkServer::remove_player_for_connection(self, RemovePlayerOptions::KeepActive);
                    } else {
                        NetworkServer::destroy(&mut identity);
                    }
                }
            }
            NetworkServerStatic::get_static_spawned_network_identities().remove(&net_id);
        }
        self.owned().clear();
    }

    pub fn remove_from_observing(&mut self, identity: &mut NetworkIdentity, is_destroyed: bool) {
        self.observing.retain(|net_id| *net_id != identity.net_id());
        if !is_destroyed {
            NetworkServer::hide_for_connection(identity, self);
        }
    }
}