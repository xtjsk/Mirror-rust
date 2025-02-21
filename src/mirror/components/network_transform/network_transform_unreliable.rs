use crate::log_error;
use crate::mirror::components::network_transform::network_transform_base::{
    CoordinateSpace, NetworkTransformBase, NetworkTransformBaseTrait,
};
use crate::mirror::components::network_transform::transform_snapshot::TransformSnapshot;
use crate::mirror::components::network_transform::transform_sync_data::{Changed, SyncData};
use crate::mirror::core::backend_data::NetworkBehaviourComponent;
use crate::mirror::core::messages::NetworkMessageTrait;
use crate::mirror::core::network_behaviour::{GameObject, NetworkBehaviour, NetworkBehaviourTrait, SyncDirection, SyncMode};
use crate::mirror::core::network_connection::NetworkConnectionTrait;
use crate::mirror::core::network_reader::{NetworkReader, NetworkReaderTrait};
use crate::mirror::core::network_server::{NetworkServerStatic, NETWORK_BEHAVIOURS};
use crate::mirror::core::network_time::NetworkTime;
use crate::mirror::core::network_writer::{NetworkWriter, NetworkWriterTrait};
use crate::mirror::core::network_writer_pool::NetworkWriterPool;
use crate::mirror::core::remote_calls::RemoteProcedureCalls;
use crate::mirror::core::snapshot_interpolation::snapshot_interpolation::SnapshotInterpolation;
use crate::mirror::core::sync_object::SyncObject;
use crate::mirror::core::tools::accurateinterval::AccurateInterval;
use crate::mirror::core::tools::compress::CompressTrait;
use crate::mirror::core::transport::TransportChannel;
use dashmap::try_result::TryResult;
use nalgebra::{Quaternion, UnitQuaternion, Vector3};
use ordered_float::OrderedFloat;
use std::any::Any;
use std::collections::BTreeMap;
use std::mem::take;
use std::sync::Once;

#[derive(Debug)]
pub struct NetworkTransformUnreliable {
    network_transform_base: NetworkTransformBase,
    buffer_reset_multiplier: f32,
    position_sensitivity: f32,
    rotation_sensitivity: f32,
    scale_sensitivity: f32,
    send_interval_counter: u32,
    last_send_interval_time: f64,

    last_snapshot: TransformSnapshot,
    #[allow(warnings)]
    cached_snapshot_comparison: bool,
    cached_changed_comparison: u8,
    has_sent_unchanged_position: bool,
}

impl NetworkTransformUnreliable {
    pub const COMPONENT_TAG: &'static str = "Mirror.NetworkTransformUnreliable";
    // UpdateServerInterpolation
    fn update_server_interpolation(&mut self) {
        if *self.sync_direction() == SyncDirection::ClientToServer
            && self.connection_to_client() != 0
        {
            if self.network_transform_base.server_snapshots.len() == 0 {
                return;
            }

            match NetworkServerStatic::network_connections().try_get(&self.connection_to_client()) {
                TryResult::Present(conn) => {
                    let (from, to, t) = SnapshotInterpolation::step_interpolation(
                        &mut self.network_transform_base.server_snapshots,
                        conn.remote_timeline,
                    );
                    let computed = TransformSnapshot::transform_snapshot(from, to, t);
                    self.apply(computed, to);
                }
                TryResult::Absent => {
                    log_error!(format!(
                        "Failed because connection {} is absent.",
                        self.connection_to_client()
                    ));
                }
                TryResult::Locked => {
                    log_error!(format!(
                        "Failed because connection {} is locked.",
                        &self.connection_to_client()
                    ));
                }
            }
        }
    }
    // UpdateServerBroadcast
    fn update_server_broadcast(&mut self) {
        self.r_check_last_send_time();

        if self.send_interval_counter == self.network_transform_base.send_interval_multiplier
            && (*self.sync_direction() == SyncDirection::ServerToClient)
        {
            let snapshot = self.construct();

            self.cached_changed_comparison = self.compare_changed_snapshots(&snapshot);

            if (self.cached_changed_comparison == Changed::None.to_u8()
                || self.cached_changed_comparison == Changed::CompressRot.to_u8())
                && self.has_sent_unchanged_position
                && self.network_transform_base.only_sync_on_change
            {
                let sync_data = SyncData::new(
                    self.cached_changed_comparison,
                    snapshot.position,
                    snapshot.rotation,
                    snapshot.scale,
                );
                self.rpc_server_to_client_sync(sync_data);

                if self.cached_changed_comparison == Changed::None.to_u8()
                    || self.cached_changed_comparison == Changed::CompressRot.to_u8()
                {
                    self.has_sent_unchanged_position = true;
                } else {
                    self.has_sent_unchanged_position = false;
                    self.update_last_sent_snapshot(self.cached_changed_comparison, snapshot);
                }
            }
        }
    }
    // CheckLastSendTime
    fn r_check_last_send_time(&mut self) {
        if self.send_interval_counter >= self.network_transform_base.send_interval_multiplier {
            self.send_interval_counter = 0;
        }

        if AccurateInterval::elapsed(
            NetworkTime::local_time(),
            NetworkServerStatic::send_interval() as f64,
            &mut self.last_send_interval_time,
        ) {
            self.send_interval_counter += 1;
        }
    }
    fn compare_changed_snapshots(&self, snapshot: &TransformSnapshot) -> u8 {
        let mut changed = Changed::None.to_u8();

        if self.sync_position() {
            let position_changed = (snapshot.position - self.last_snapshot.position)
                .magnitude_squared()
                > self.position_sensitivity * self.position_sensitivity;
            if position_changed {
                if (self.last_snapshot.position.x - snapshot.position.x).abs()
                    > self.position_sensitivity
                {
                    changed |= Changed::PosX.to_u8();
                }
                if (self.last_snapshot.position.y - snapshot.position.y).abs()
                    > self.position_sensitivity
                {
                    changed |= Changed::PosY.to_u8();
                }
                if (self.last_snapshot.position.z - snapshot.position.z).abs()
                    > self.position_sensitivity
                {
                    changed |= Changed::PosZ.to_u8();
                }
            }
        }

        if self.sync_rotation() {
            if self.network_transform_base.compress_rotation {
                let rotation_changed = UnitQuaternion::from_quaternion(self.last_snapshot.rotation)
                    .angle_to(&UnitQuaternion::from_quaternion(snapshot.rotation))
                    .to_degrees()
                    > self.rotation_sensitivity;
                if rotation_changed {
                    changed |= Changed::CompressRot.to_u8();
                    changed |= Changed::Rot.to_u8();
                } else {
                    changed |= Changed::CompressRot.to_u8();
                }
            } else {
                if (self.last_snapshot.rotation.coords.x - snapshot.rotation.coords.x).abs()
                    > self.rotation_sensitivity
                {
                    changed |= Changed::RotX.to_u8();
                }
                if (self.last_snapshot.rotation.coords.y - snapshot.rotation.coords.y).abs()
                    > self.rotation_sensitivity
                {
                    changed |= Changed::RotY.to_u8();
                }
                if (self.last_snapshot.rotation.coords.z - snapshot.rotation.coords.z).abs()
                    > self.rotation_sensitivity
                {
                    changed |= Changed::RotZ.to_u8();
                }
            }
        }

        if self.sync_scale() {
            if (self.last_snapshot.scale - snapshot.scale).magnitude_squared()
                > self.scale_sensitivity * self.scale_sensitivity
            {
                changed |= Changed::Scale.to_u8();
            }
        }
        changed
    }
    fn update_last_sent_snapshot(&mut self, changed: u8, current_snapshot: TransformSnapshot) {
        if changed == Changed::None.to_u8() || changed == Changed::CompressRot.to_u8() {
            return;
        }

        if changed & Changed::PosX.to_u8() > 0 {
            self.last_snapshot.position.x = current_snapshot.position.x;
        }
        if changed & Changed::PosY.to_u8() > 0 {
            self.last_snapshot.position.y = current_snapshot.position.y;
        }
        if changed & Changed::PosZ.to_u8() > 0 {
            self.last_snapshot.position.z = current_snapshot.position.z;
        }

        if self.network_transform_base.compress_rotation {
            if changed & Changed::Rot.to_u8() > 0 {
                self.last_snapshot.rotation = current_snapshot.rotation;
            }
        } else {
            let euler_angles =
                UnitQuaternion::from_quaternion(self.last_snapshot.rotation).euler_angles();
            let mut new_rotation = Vector3::new(euler_angles.0, euler_angles.1, euler_angles.2);
            if changed & Changed::RotX.to_u8() > 0 {
                new_rotation.x = UnitQuaternion::from_quaternion(current_snapshot.rotation)
                    .euler_angles()
                    .0;
            }
            if changed & Changed::RotY.to_u8() > 0 {
                new_rotation.y = UnitQuaternion::from_quaternion(current_snapshot.rotation)
                    .euler_angles()
                    .1;
            }
            if changed & Changed::RotZ.to_u8() > 0 {
                new_rotation.z = UnitQuaternion::from_quaternion(current_snapshot.rotation)
                    .euler_angles()
                    .2;
            }
            self.last_snapshot.rotation =
                *UnitQuaternion::from_euler_angles(new_rotation.x, new_rotation.y, new_rotation.z)
                    .quaternion();
        }

        if changed & Changed::Scale.to_u8() > 0 {
            self.last_snapshot.scale = current_snapshot.scale;
        }
    }
    // InvokeUserCode_CmdClientToServerSync__Nullable\u00601__Nullable\u00601__Nullable\u00601
    fn invoke_user_code_cmd_client_to_server_sync_nullable_1_nullable_1_nullable_1(
        _conn_id: u64,
        net_id: u32,
        component_index: u8,
        _func_hash: u16,
        reader: &mut NetworkReader,
    ) {
        if !NetworkServerStatic::active() {
            log_error!("Command CmdClientToServerSync called on client.");
            return;
        }
        // 获取 NetworkBehaviour
        match NETWORK_BEHAVIOURS.try_get_mut(&format!("{}_{}", net_id, component_index)) {
            TryResult::Present(mut component) => {
                component
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap()
                    .user_code_cmd_client_to_server_sync_nullable_1_nullable_1_nullable_1(
                        reader.read_vector3_nullable(),
                        reader.read_quaternion_nullable(),
                        reader.read_vector3_nullable(),
                    );
                NetworkBehaviour::late_invoke(net_id, component.game_object().clone());
            }
            TryResult::Absent => {
                log_error!(
                    "NetworkBehaviour not found by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
            TryResult::Locked => {
                log_error!(
                    "NetworkBehaviour locked by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
        }
    }

    // UserCode_CmdClientToServerSync__Nullable\u00601__Nullable\u00601__Nullable\u00601
    fn user_code_cmd_client_to_server_sync_nullable_1_nullable_1_nullable_1(
        &mut self,
        position: Option<Vector3<f32>>,
        rotation: Option<Quaternion<f32>>,
        scale: Option<Vector3<f32>>,
    ) {
        self.on_client_to_server_sync_nullable_1_nullable_1_nullable_1(position, rotation, scale);
        if *self.sync_direction() != SyncDirection::ClientToServer {
            return;
        }
        self.rpc_server_to_client_sync_nullable_1_nullable_1_nullable_1(position, rotation, scale);
    }

    fn invoke_user_code_cmd_client_to_server_sync_compress_rotation_nullable_1_nullable_1_nullable_1(
        _conn_id: u64,
        net_id: u32,
        component_index: u8,
        _func_hash: u16,
        reader: &mut NetworkReader,
    ) {
        if !NetworkServerStatic::active() {
            log_error!("Command CmdClientToServerSync called on client.");
            return;
        }

        // 获取 NetworkBehaviour
        match NETWORK_BEHAVIOURS.try_get_mut(&format!("{}_{}", net_id, component_index)) {
            TryResult::Present(mut component) => {
                component
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap()
                    .user_code_cmd_client_to_server_sync_compress_rotation_nullable_1_nullable_1_nullable_1(
                        reader.read_vector3_nullable(),
                        reader.read_uint_nullable(),
                        reader.read_vector3_nullable(),
                    );
                NetworkBehaviour::late_invoke(net_id, component.game_object().clone());
            }
            TryResult::Absent => {
                log_error!(
                    "NetworkBehaviour not found by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
            TryResult::Locked => {
                log_error!(
                    "NetworkBehaviour locked by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
        }
    }

    fn user_code_cmd_client_to_server_sync_compress_rotation_nullable_1_nullable_1_nullable_1(
        &mut self,
        position: Option<Vector3<f32>>,
        rotation: Option<u32>,
        scale: Option<Vector3<f32>>,
    ) {
        let quaternion: Option<Quaternion<f32>>;
        if rotation.is_none() {
            if self.network_transform_base.server_snapshots.len() > 0 {
                let (_, last_snapshot) = self
                    .network_transform_base
                    .server_snapshots
                    .iter()
                    .last()
                    .unwrap();
                quaternion = Some(last_snapshot.rotation);
            } else {
                quaternion = Some(self.get_rotation());
            }
        } else {
            quaternion = Some(Quaternion::decompress(rotation.unwrap()));
        }
        self.on_client_to_server_sync_nullable_1_nullable_1_nullable_1(position, quaternion, scale);
    }

    // &mut Box<dyn NetworkBehaviourTrait>, &mut NetworkReader, u64
    fn invoke_user_code_cmd_client_to_server_sync_sync_data(
        _conn_id: u64,
        net_id: u32,
        component_index: u8,
        _func_hash: u16,
        reader: &mut NetworkReader,
    ) {
        if !NetworkServerStatic::active() {
            log_error!("Command CmdClientToServerSync called on client.");
            return;
        }
        let sync_data = SyncData::deserialize(reader);

        // 获取 NetworkBehaviour
        match NETWORK_BEHAVIOURS.try_get_mut(&format!("{}_{}", net_id, component_index)) {
            TryResult::Present(mut component) => {
                component
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap()
                    .user_code_cmd_client_to_server_sync_sync_data(sync_data);
                NetworkBehaviour::late_invoke(net_id, component.game_object().clone());
            }
            TryResult::Absent => {
                log_error!(
                    "NetworkBehaviour not found by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
            TryResult::Locked => {
                log_error!(
                    "NetworkBehaviour locked by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
        }
    }

    // UserCode_CmdClientToServerSync__SyncData
    fn user_code_cmd_client_to_server_sync_sync_data(&mut self, sync_data: SyncData) {
        self.on_client_to_server_sync(sync_data);
        if *self.sync_direction() != SyncDirection::ClientToServer {
            return;
        }
        self.rpc_server_to_client_sync(sync_data);
    }

    // OnClientToServerSync(
    // Vector3? position,
    // Quaternion? rotation,
    // Vector3? scale)
    fn on_client_to_server_sync_nullable_1_nullable_1_nullable_1(
        &mut self,
        position: Option<Vector3<f32>>,
        rotation: Option<Quaternion<f32>>,
        scale: Option<Vector3<f32>>,
    ) {
        // only apply if in client authority mode
        if *self.sync_direction() != SyncDirection::ClientToServer {
            return;
        }

        let mut timestamp = 0f64;
        match NetworkServerStatic::network_connections().try_get(&self.connection_to_client()) {
            TryResult::Present(conn) => {
                if self.network_transform_base.server_snapshots.len()
                    >= conn.snapshot_buffer_size_limit as usize
                {
                    return;
                }
                timestamp = conn.remote_time_stamp();
            }
            TryResult::Absent => {
                log_error!(format!(
                    "Failed because connection {} is absent.",
                    self.connection_to_client()
                ));
            }
            TryResult::Locked => {
                log_error!(format!(
                    "Failed because connection {} is locked.",
                    &self.connection_to_client()
                ));
            }
        }

        if self.network_transform_base.only_sync_on_change {
            let time_interval_check = self.buffer_reset_multiplier as f64
                * self.network_transform_base.send_interval_multiplier as f64
                * NetworkServerStatic::send_interval() as f64;

            if let Some((_, last_snapshot)) =
                self.network_transform_base.server_snapshots.iter().last()
            {
                if last_snapshot.remote_time + time_interval_check < timestamp {
                    self.network_transform_base.reset_state();
                }
            }
        }
        let mut server_snapshots = take(&mut self.network_transform_base.server_snapshots);
        self.add_snapshot(&mut server_snapshots, timestamp, position, rotation, scale);
        self.network_transform_base.server_snapshots = server_snapshots;
    }

    // void OnClientToServerSync
    fn on_client_to_server_sync(&mut self, mut sync_data: SyncData) {
        // only apply if in client authority mode
        if *self.sync_direction() != SyncDirection::ClientToServer {
            return;
        }

        let mut timestamp = 0f64;
        match NetworkServerStatic::network_connections().try_get(&self.connection_to_client()) {
            TryResult::Present(conn) => {
                if self.network_transform_base.server_snapshots.len()
                    >= conn.snapshot_buffer_size_limit as usize
                {
                    return;
                }
                timestamp = conn.remote_time_stamp();
            }
            TryResult::Absent => {
                log_error!(format!(
                    "Failed because connection {} is absent.",
                    self.connection_to_client()
                ));
            }
            TryResult::Locked => {
                log_error!(format!(
                    "Failed because connection {} is locked.",
                    &self.connection_to_client()
                ));
            }
        }

        if self.network_transform_base.only_sync_on_change {
            let time_interval_check = self.buffer_reset_multiplier as f64
                * self.network_transform_base.send_interval_multiplier as f64
                * NetworkServerStatic::send_interval() as f64;

            if let Some((_, last_snapshot)) =
                self.network_transform_base.server_snapshots.iter().last()
            {
                if last_snapshot.remote_time + time_interval_check < timestamp {
                    self.network_transform_base.reset_state();
                }
            }
        }
        self.update_sync_data(
            &mut sync_data,
            &self.network_transform_base.server_snapshots,
        );
        let mut server_snapshots = take(&mut self.network_transform_base.server_snapshots);
        self.add_snapshot(
            &mut server_snapshots,
            timestamp
                + self.network_transform_base.time_stamp_adjustment
                + self.network_transform_base.offset,
            Some(sync_data.position),
            Some(sync_data.quat_rotation),
            Some(sync_data.scale),
        );
        self.network_transform_base.server_snapshots = server_snapshots;
    }

    // void UpdateSyncData
    fn update_sync_data(
        &self,
        sync_data: &mut SyncData,
        snapshots: &BTreeMap<OrderedFloat<f64>, TransformSnapshot>,
    ) {
        if sync_data.changed_data_byte == Changed::None.to_u8()
            || sync_data.changed_data_byte == Changed::CompressRot.to_u8()
        {
            if let Some((_, last_snapshot)) = snapshots.iter().last() {
                sync_data.position = last_snapshot.position;
                sync_data.quat_rotation = last_snapshot.rotation;
                sync_data.scale = last_snapshot.scale;
            } else {
                sync_data.position = self.get_position();
                sync_data.quat_rotation = self.get_rotation();
                sync_data.scale = self.get_scale();
            }
        } else {
            // x
            if sync_data.changed_data_byte & Changed::PosX.to_u8() <= 0 {
                if let Some((_, last_snapshot)) = snapshots.iter().last() {
                    sync_data.position.x = last_snapshot.position.x;
                } else {
                    sync_data.position.x = self.get_position().x;
                }
            }
            // y
            if sync_data.changed_data_byte & Changed::PosY.to_u8() <= 0 {
                if let Some((_, last_snapshot)) = snapshots.iter().last() {
                    sync_data.position.y = last_snapshot.position.y;
                } else {
                    sync_data.position.y = self.get_position().y;
                }
            }
            // z
            if sync_data.changed_data_byte & Changed::PosZ.to_u8() <= 0 {
                if let Some((_, last_snapshot)) = snapshots.iter().last() {
                    sync_data.position.z = last_snapshot.position.z;
                } else {
                    sync_data.position.z = self.get_position().z;
                }
            }

            if sync_data.changed_data_byte & Changed::CompressRot.to_u8() == 0 {
                // Rot x
                if sync_data.changed_data_byte & Changed::RotX.to_u8() <= 0 {
                    if let Some((_, last_snapshot)) = snapshots.iter().last() {
                        let euler_angles =
                            UnitQuaternion::from_quaternion(last_snapshot.rotation).euler_angles();
                        sync_data.vec_rotation.x = euler_angles.0;
                    } else {
                        let euler_angles =
                            UnitQuaternion::from_quaternion(self.get_rotation()).euler_angles();
                        sync_data.vec_rotation.x = euler_angles.0;
                    }
                }
                // Rot y
                if sync_data.changed_data_byte & Changed::RotY.to_u8() <= 0 {
                    if let Some((_, last_snapshot)) = snapshots.iter().last() {
                        let euler_angles =
                            UnitQuaternion::from_quaternion(last_snapshot.rotation).euler_angles();
                        sync_data.vec_rotation.y = euler_angles.1;
                    } else {
                        let euler_angles =
                            UnitQuaternion::from_quaternion(self.get_rotation()).euler_angles();
                        sync_data.vec_rotation.y = euler_angles.1;
                    }
                }
                // Rot z
                if sync_data.changed_data_byte & Changed::RotZ.to_u8() <= 0 {
                    if let Some((_, last_snapshot)) = snapshots.iter().last() {
                        let euler_angles =
                            UnitQuaternion::from_quaternion(last_snapshot.rotation).euler_angles();
                        sync_data.vec_rotation.z = euler_angles.2;
                    } else {
                        let euler_angles =
                            UnitQuaternion::from_quaternion(self.get_rotation()).euler_angles();
                        sync_data.vec_rotation.z = euler_angles.2;
                    }
                }
            } else {
                if sync_data.changed_data_byte & Changed::CompressRot.to_u8() <= 0 {
                    if let Some((_, last_snapshot)) = snapshots.iter().last() {
                        sync_data.quat_rotation = last_snapshot.rotation;
                    } else {
                        sync_data.quat_rotation = self.get_rotation();
                    }
                }
            }
            if sync_data.changed_data_byte & Changed::Scale.to_u8() <= 0 {
                if let Some((_, last_snapshot)) = snapshots.iter().last() {
                    sync_data.scale = last_snapshot.scale;
                } else {
                    sync_data.scale = self.get_scale();
                }
            }
        }
    }

    // RpcServerToClientSync
    // [ClientRpc(channel = Un)]
    fn rpc_server_to_client_sync(&mut self, mut sync_data: SyncData) {
        NetworkWriterPool::get_return(|writer| {
            sync_data.serialize(writer);
            self.send_rpc_internal(
                "System.Void Mirror.NetworkTransformUnreliable::RpcServerToClientSync(Mirror.SyncData)",
                -1891602648,
                writer,
                TransportChannel::Unreliable,
                true,
            );
        });
    }

    // RpcServerToClientSync(Vector3? position, Quaternion? rotation, Vector3? scale)
    fn rpc_server_to_client_sync_nullable_1_nullable_1_nullable_1(
        &mut self,
        position: Option<Vector3<f32>>,
        rotation: Option<Quaternion<f32>>,
        scale: Option<Vector3<f32>>,
    ) {
        NetworkWriterPool::get_return(|writer| {
            writer.write_vector3_nullable(position);
            writer.write_quaternion_nullable(rotation);
            writer.write_vector3_nullable(scale);
            self.send_rpc_internal(
                "System.Void Mirror.NetworkTransformUnreliable::RpcServerToClientSync(System.Nullable`1<UnityEngine.Vector3>,System.Nullable`1<UnityEngine.Quaternion>,System.Nullable`1<UnityEngine.Vector3>)",
                1202296400,
                writer,
                TransportChannel::Unreliable,
                true,
            );
        });
    }

    // NetworkTransformBase start

    // InvokeUserCode_CmdTeleport__Vector3
    fn invoke_user_code_cmd_teleport_vector3(
        _conn_id: u64,
        net_id: u32,
        component_index: u8,
        _func_hash: u16,
        reader: &mut NetworkReader,
    ) {
        if !NetworkServerStatic::active() {
            log_error!("Command CmdTeleport called on client.");
            return;
        }

        // 获取 NetworkBehaviour
        match NETWORK_BEHAVIOURS.try_get_mut(&format!("{}_{}", net_id, component_index)) {
            TryResult::Present(mut component) => {
                component
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap()
                    .user_code_cmd_teleport_vector3(reader.read_vector3());
                NetworkBehaviour::late_invoke(net_id, component.game_object().clone());
            }
            TryResult::Absent => {
                log_error!(
                    "NetworkBehaviour not found by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
            TryResult::Locked => {
                log_error!(
                    "NetworkBehaviour locked by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
        }
    }

    // UserCode_CmdTeleport_Vector3
    fn user_code_cmd_teleport_vector3(&mut self, position: Vector3<f32>) {
        if *self.sync_direction() != SyncDirection::ServerToClient {
            return;
        }
        self.on_teleport_vector3(position);
        self.rpc_teleport_vector3(position);
    }

    fn on_teleport_vector3(&mut self, position: Vector3<f32>) {
        self.set_position(position);
        self.reset_state();
    }

    fn rpc_teleport_vector3(&mut self, position: Vector3<f32>) {
        NetworkWriterPool::get_return(|writer| {
            writer.write_vector3(position);
            self.send_rpc_internal(
                "System.Void Mirror.NetworkTransformBase::RpcTeleport(UnityEngine.Vector3)",
                -1933368736,
                writer,
                TransportChannel::Reliable,
                true,
            );
        });
    }

    // InvokeUserCode_CmdTeleport__Vector3__Quaternion
    fn invoke_user_code_cmd_teleport_vector3_quaternion(
        _conn_id: u64,
        net_id: u32,
        component_index: u8,
        _func_hash: u16,
        reader: &mut NetworkReader,
    ) {
        if !NetworkServerStatic::active() {
            log_error!("Command CmdTeleport called on client.");
            return;
        }

        // 获取 NetworkBehaviour
        match NETWORK_BEHAVIOURS.try_get_mut(&format!("{}_{}", net_id, component_index)) {
            TryResult::Present(mut component) => {
                component
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap()
                    .user_code_cmd_teleport_vector3_quaternion(
                        reader.read_vector3(),
                        reader.read_quaternion(),
                    );
                NetworkBehaviour::late_invoke(net_id, component.game_object().clone());
            }
            TryResult::Absent => {
                log_error!(
                    "NetworkBehaviour not found by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
            TryResult::Locked => {
                log_error!(
                    "NetworkBehaviour locked by net_id: {}, component_index: {}",
                    net_id,
                    component_index
                );
            }
        }
    }

    // UserCode_CmdTeleport_Vector3_Quaternion
    fn user_code_cmd_teleport_vector3_quaternion(
        &mut self,
        position: Vector3<f32>,
        rotation: Quaternion<f32>,
    ) {
        if *self.sync_direction() != SyncDirection::ServerToClient {
            return;
        }
        self.on_teleport_vector3_quaternion(position, rotation);
        self.rpc_teleport_vector3_quaternion(position, rotation);
    }

    fn on_teleport_vector3_quaternion(
        &mut self,
        position: Vector3<f32>,
        rotation: Quaternion<f32>,
    ) {
        self.set_position(position);
        self.set_rotation(rotation);
        self.reset_state();
    }

    fn rpc_teleport_vector3_quaternion(
        &mut self,
        position: Vector3<f32>,
        rotation: Quaternion<f32>,
    ) {
        NetworkWriterPool::get_return(|writer| {
            writer.write_vector3(position);
            writer.write_quaternion(rotation);
            self.send_rpc_internal(
                "System.Void Mirror.NetworkTransformBase::RpcTeleport(UnityEngine.Vector3,UnityEngine.Quaternion)",
                -1675599861,
                writer,
                TransportChannel::Reliable,
                true,
            );
        });
    }
}

impl NetworkBehaviourTrait for NetworkTransformUnreliable {
    fn new(
        game_object: GameObject,
        network_behaviour_component: &NetworkBehaviourComponent,
    ) -> Self {
        Self::call_register_delegate();
        NetworkTransformUnreliable {
            network_transform_base: NetworkTransformBase::new(
                game_object,
                network_behaviour_component.network_transform_base_setting,
                network_behaviour_component.network_behaviour_setting,
                network_behaviour_component.index,
                network_behaviour_component.sub_class.clone(),
            ),
            buffer_reset_multiplier: network_behaviour_component
                .network_transform_unreliable_setting
                .buffer_reset_multiplier,
            position_sensitivity: network_behaviour_component
                .network_transform_unreliable_setting
                .position_sensitivity,
            rotation_sensitivity: network_behaviour_component
                .network_transform_unreliable_setting
                .rotation_sensitivity,
            scale_sensitivity: network_behaviour_component
                .network_transform_unreliable_setting
                .scale_sensitivity,
            send_interval_counter: 0,
            last_send_interval_time: f64::MAX,
            last_snapshot: TransformSnapshot::default(),
            cached_snapshot_comparison: false,
            cached_changed_comparison: Changed::None.to_u8(),
            has_sent_unchanged_position: false,
        }
    }

    fn register_delegate()
    where
        Self: Sized,
    {
        // System.Void Mirror.NetworkTransformUnreliable::CmdClientToServerSync(System.Nullable`1<UnityEngine.Vector3>,System.Nullable`1<UnityEngine.Quaternion>,System.Nullable`1<UnityEngine.Vector3>)
        RemoteProcedureCalls::register_command_delegate::<Self>(
            "System.Void Mirror.NetworkTransformUnreliable::CmdClientToServerSync(System.Nullable`1<UnityEngine.Vector3>,System.Nullable`1<UnityEngine.Quaternion>,System.Nullable`1<UnityEngine.Vector3>)",
            Self::invoke_user_code_cmd_client_to_server_sync_nullable_1_nullable_1_nullable_1,
            true,
        );

        // System.Void Mirror.NetworkTransformUnreliable::CmdClientToServerSyncCompressRotation(System.Nullable`1<UnityEngine.Vector3>,System.Nullable`1<System.UInt32>,System.Nullable`1<UnityEngine.Vector3>)
        RemoteProcedureCalls::register_command_delegate::<Self>(
            "System.Void Mirror.NetworkTransformUnreliable::CmdClientToServerSyncCompressRotation(System.Nullable`1<UnityEngine.Vector3>,System.Nullable`1<System.UInt32>,System.Nullable`1<UnityEngine.Vector3>)",
            Self::invoke_user_code_cmd_client_to_server_sync_compress_rotation_nullable_1_nullable_1_nullable_1,
            true,
        );

        // System.Void Mirror.NetworkTransformUnreliable::CmdClientToServerSync(Mirror.SyncData)
        RemoteProcedureCalls::register_command_delegate::<Self>(
            "System.Void Mirror.NetworkTransformUnreliable::CmdClientToServerSync(Mirror.SyncData)",
            Self::invoke_user_code_cmd_client_to_server_sync_sync_data,
            true,
        );

        // System.Void Mirror.NetworkTransformBase::CmdTeleport(UnityEngine.Vector3)
        RemoteProcedureCalls::register_command_delegate::<Self>(
            "System.Void Mirror.NetworkTransformBase::CmdTeleport(UnityEngine.Vector3)",
            Self::invoke_user_code_cmd_teleport_vector3,
            true,
        );

        // System.Void Mirror.NetworkTransformBase::CmdTeleport(UnityEngine.Vector3,UnityEngine.Quaternion)
        RemoteProcedureCalls::register_command_delegate::<Self>(
            "System.Void Mirror.NetworkTransformBase::CmdTeleport(UnityEngine.Vector3,UnityEngine.Quaternion)",
            Self::invoke_user_code_cmd_teleport_vector3_quaternion,
            true,
        );
    }

    fn get_once() -> &'static Once
    where
        Self: Sized,
    {
        static ONCE: Once = Once::new();
        &ONCE
    }

    fn sync_interval(&self) -> f64 {
        self.network_transform_base.network_behaviour.sync_interval
    }

    fn set_sync_interval(&mut self, value: f64) {
        self.network_transform_base.network_behaviour.sync_interval = value
    }

    fn last_sync_time(&self) -> f64 {
        self.network_transform_base.network_behaviour.last_sync_time
    }

    fn set_last_sync_time(&mut self, value: f64) {
        self.network_transform_base.network_behaviour.last_sync_time = value
    }

    fn sync_direction(&mut self) -> &SyncDirection {
        &self.network_transform_base.network_behaviour.sync_direction
    }

    fn set_sync_direction(&mut self, value: SyncDirection) {
        self.network_transform_base.network_behaviour.sync_direction = value
    }

    fn sync_mode(&mut self) -> &SyncMode {
        &self.network_transform_base.network_behaviour.sync_mode
    }

    fn set_sync_mode(&mut self, value: SyncMode) {
        self.network_transform_base.network_behaviour.sync_mode = value
    }

    fn index(&self) -> u8 {
        self.network_transform_base.network_behaviour.index
    }

    fn set_index(&mut self, value: u8) {
        self.network_transform_base.network_behaviour.index = value
    }

    fn sub_class(&self) -> String {
        self.network_transform_base
            .network_behaviour
            .sub_class
            .clone()
    }

    fn set_sub_class(&mut self, value: String) {
        self.network_transform_base.network_behaviour.sub_class = value
    }

    fn sync_var_dirty_bits(&self) -> u64 {
        self.network_transform_base
            .network_behaviour
            .sync_var_dirty_bits
    }

    fn __set_sync_var_dirty_bits(&mut self, value: u64) {
        self.network_transform_base
            .network_behaviour
            .sync_var_dirty_bits = value
    }

    fn sync_object_dirty_bits(&self) -> u64 {
        self.network_transform_base
            .network_behaviour
            .sync_object_dirty_bits
    }

    fn __set_sync_object_dirty_bits(&mut self, value: u64) {
        self.network_transform_base
            .network_behaviour
            .sync_object_dirty_bits = value
    }

    fn net_id(&self) -> u32 {
        self.network_transform_base.network_behaviour.net_id
    }

    fn set_net_id(&mut self, value: u32) {
        self.network_transform_base.network_behaviour.net_id = value
    }

    fn connection_to_client(&self) -> u64 {
        self.network_transform_base
            .network_behaviour
            .connection_to_client
    }

    fn set_connection_to_client(&mut self, value: u64) {
        self.network_transform_base
            .network_behaviour
            .connection_to_client = value
    }

    fn observers(&self) -> &Vec<u64> {
        &self.network_transform_base.network_behaviour.observers
    }

    fn add_observer(&mut self, conn_id: u64) {
        self.network_transform_base
            .network_behaviour
            .observers
            .push(conn_id);
    }

    fn remove_observer(&mut self, value: u64) {
        self.network_transform_base
            .network_behaviour
            .observers
            .retain(|&x| x != value);
    }


    fn game_object(&self) -> &GameObject {
        &self.network_transform_base.network_behaviour.game_object
    }

    fn set_game_object(&mut self, value: GameObject) {
        self.network_transform_base.network_behaviour.game_object = value
    }

    fn sync_objects(&mut self) -> &mut Vec<Box<dyn SyncObject>> {
        &mut self.network_transform_base.network_behaviour.sync_objects
    }

    fn set_sync_objects(&mut self, value: Vec<Box<dyn SyncObject>>) {
        self.network_transform_base.network_behaviour.sync_objects = value
    }

    fn add_sync_object(&mut self, value: Box<dyn SyncObject>) {
        self.network_transform_base
            .network_behaviour
            .sync_objects
            .push(value);
    }

    fn sync_var_hook_guard(&self) -> u64 {
        self.network_transform_base
            .network_behaviour
            .sync_var_hook_guard
    }

    fn __set_sync_var_hook_guard(&mut self, value: u64) {
        self.network_transform_base
            .network_behaviour
            .sync_var_hook_guard = value
    }

    fn is_dirty(&self) -> bool {
        self.network_transform_base.network_behaviour.is_dirty()
    }
    fn on_serialize(&mut self, writer: &mut NetworkWriter, initial_state: bool) {
        if initial_state {
            if self.network_transform_base.sync_position {
                writer.write_vector3(self.get_position());
            }
            if self.network_transform_base.sync_rotation {
                writer.write_quaternion(self.get_rotation());
            }
            if self.network_transform_base.sync_scale {
                writer.write_vector3(self.get_scale());
            }
        }
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn update(&mut self) {
        self.update_server_interpolation();
    }

    fn late_update(&mut self) {
        self.update_server_broadcast();
    }

    fn serialize_sync_vars(&mut self, _writer: &mut NetworkWriter, _initial_state: bool) {}

    fn deserialize_sync_vars(&mut self, _reader: &mut NetworkReader, _initial_state: bool) -> bool {
        true
    }
}

impl NetworkTransformBaseTrait for NetworkTransformUnreliable {
    fn coordinate_space(&self) -> &CoordinateSpace {
        &self.network_transform_base.coordinate_space
    }

    fn set_coordinate_space(&mut self, value: CoordinateSpace) {
        self.network_transform_base.coordinate_space = value;
    }

    fn get_game_object(&self) -> &GameObject {
        &self.network_transform_base.network_behaviour.game_object
    }

    fn set_game_object(&mut self, value: GameObject) {
        self.network_transform_base.network_behaviour.game_object = value;
    }

    fn sync_position(&self) -> bool {
        self.network_transform_base.sync_position
    }

    fn sync_rotation(&self) -> bool {
        self.network_transform_base.sync_rotation
    }

    fn interpolate_position(&self) -> bool {
        self.network_transform_base.interpolate_position
    }

    fn interpolate_rotation(&self) -> bool {
        self.network_transform_base.interpolate_rotation
    }

    fn interpolate_scale(&self) -> bool {
        self.network_transform_base.interpolate_scale
    }

    fn sync_scale(&self) -> bool {
        self.network_transform_base.sync_scale
    }

    fn reset_state(&mut self) {
        self.network_transform_base.reset_state();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_network_behaviour_trait() {}
}
