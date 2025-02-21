use crate::log_error;
use crate::mirror::components::network_transform::network_transform_base::{
    CoordinateSpace, NetworkTransformBase, NetworkTransformBaseTrait,
};
use crate::mirror::components::network_transform::transform_snapshot::TransformSnapshot;
use crate::mirror::core::backend_data::NetworkBehaviourComponent;
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
use crate::mirror::core::tools::compress::{Compress, CompressTrait};
use crate::mirror::core::tools::delta_compression::DeltaCompression;
use crate::mirror::core::transport::TransportChannel;
use dashmap::try_result::TryResult;
use nalgebra::{Quaternion, UnitQuaternion, Vector3};
use ordered_float::OrderedFloat;
use std::any::Any;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::mem::take;
use std::sync::Once;

#[derive(Debug)]
pub struct NetworkTransformReliable {
    network_transform_base: NetworkTransformBase,

    // NetworkTransformReliableSetting start
    only_sync_on_change_correction_multiplier: f32,
    rotation_sensitivity: f32,
    position_precision: f32,
    scale_precision: f32,
    compress_rotation: bool,
    // NetworkTransformReliableSetting end
    send_interval_counter: u32,
    last_send_interval_time: f64,
    last_snapshot: TransformSnapshot,
    last_serialized_position: Vector3<i64>,
    last_deserialized_position: Vector3<i64>,
    last_serialized_scale: Vector3<i64>,
    last_deserialized_scale: Vector3<i64>,
}

impl NetworkTransformReliable {
    pub const COMPONENT_TAG: &'static str = "Mirror.NetworkTransformReliable";

    // UpdateServer()
    fn update_server(&mut self) {
        if self.sync_direction() == &SyncDirection::ClientToServer
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
                        "connection not found: {}",
                        self.connection_to_client()
                    ));
                }
                TryResult::Locked => {
                    log_error!(format!(
                        "connection locked: {}",
                        self.connection_to_client()
                    ));
                }
            }
        }
    }

    fn changed(&self, current: TransformSnapshot) -> bool {
        // 最后一次快照的旋转
        let last_rotation = UnitQuaternion::from_quaternion(self.last_snapshot.rotation);
        // 当前快照的旋转
        let current_rotation = UnitQuaternion::from_quaternion(current.rotation);
        // 计算角度差异
        let angle = last_rotation.angle_to(&current_rotation);
        Self::quantized_changed(
            self.last_snapshot.position,
            current.position,
            self.position_precision,
        ) || angle > self.rotation_sensitivity
            || Self::quantized_changed(
            self.last_snapshot.scale,
            current.scale,
            self.scale_precision,
        )
    }

    fn quantized_changed(u: Vector3<f32>, v: Vector3<f32>, precision: f32) -> bool {
        let u_quantized = Compress::vector3float_to_vector3long(u, precision);
        let v_quantized = Compress::vector3float_to_vector3long(v, precision);
        u_quantized != v_quantized
    }

    // CheckLastSendTime
    fn u_check_last_send_time(&mut self) {
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

    // OnClientToServerSync()
    fn on_client_to_server_sync(
        &mut self,
        position: Vector3<f32>,
        rotation: Quaternion<f32>,
        scale: Vector3<f32>,
    ) {
        if self.sync_direction() != &SyncDirection::ClientToServer {
            return;
        }

        let mut timestamp = 0f64;
        let mut buffer_time_multiplier: f64 = 2.0;
        match NetworkServerStatic::network_connections().try_get(&self.connection_to_client()) {
            TryResult::Present(conn) => {
                if self.network_transform_base.server_snapshots.len()
                    >= conn.snapshot_buffer_size_limit as usize
                {
                    return;
                }
                timestamp = conn.remote_time_stamp();
                buffer_time_multiplier = conn.buffer_time_multiplier;
            }
            TryResult::Absent => {
                log_error!(format!(
                    "connection not found: {}",
                    self.connection_to_client()
                ));
            }
            TryResult::Locked => {
                log_error!(format!(
                    "connection locked: {}",
                    self.connection_to_client()
                ));
            }
        }

        if self.network_transform_base.only_sync_on_change
            && Self::needs_correction(
            &mut self.network_transform_base.server_snapshots,
            timestamp,
            NetworkServerStatic::send_interval() as f64
                * self.network_transform_base.send_interval_multiplier as f64,
            self.only_sync_on_change_correction_multiplier as f64,
        )
        {
            let position = self.get_position();
            let rotation = self.get_rotation();
            let scale = self.get_scale();
            Self::rewrite_history(
                &mut self.network_transform_base.server_snapshots,
                timestamp,
                NetworkTime::local_time(),
                NetworkServerStatic::send_interval() as f64
                    * self.network_transform_base.send_interval_multiplier as f64,
                position,
                rotation,
                scale,
                buffer_time_multiplier as usize,
            );
        }

        let mut server_snapshots = take(&mut self.network_transform_base.server_snapshots);
        self.add_snapshot(
            &mut server_snapshots,
            timestamp
                + self.network_transform_base.time_stamp_adjustment
                + self.network_transform_base.offset,
            Some(position),
            Some(rotation),
            Some(scale),
        );
        self.network_transform_base.server_snapshots = server_snapshots;
    }

    fn needs_correction(
        snapshots: &mut BTreeMap<OrderedFloat<f64>, TransformSnapshot>,
        remote_timestamp: f64,
        buffer_time: f64,
        tolerance_multiplier: f64,
    ) -> bool {
        snapshots.len() == 1
            && remote_timestamp - snapshots.iter().next().unwrap().1.remote_time
            >= buffer_time * tolerance_multiplier
    }

    fn rewrite_history(
        snapshots: &mut BTreeMap<OrderedFloat<f64>, TransformSnapshot>,
        remote_timestamp: f64,
        local_time: f64,
        send_interval: f64,
        position: Vector3<f32>,
        rotation: Quaternion<f32>,
        scale: Vector3<f32>,
        buffer_time_multiplier: usize,
    ) {
        snapshots.clear();
        let snapshot = TransformSnapshot::new(
            remote_timestamp - send_interval,
            local_time - send_interval,
            position,
            rotation,
            scale,
        );
        SnapshotInterpolation::insert_if_not_exists(snapshots, buffer_time_multiplier, snapshot);
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

impl NetworkBehaviourTrait for NetworkTransformReliable {
    fn new(game_object: GameObject, network_behaviour_component: &NetworkBehaviourComponent) -> Self
    where
        Self: Sized,
    {
        Self::call_register_delegate();
        Self {
            network_transform_base: NetworkTransformBase::new(
                game_object,
                network_behaviour_component.network_transform_base_setting,
                network_behaviour_component.network_behaviour_setting,
                network_behaviour_component.index,
                network_behaviour_component.sub_class.clone(),
            ),
            only_sync_on_change_correction_multiplier: network_behaviour_component
                .network_transform_reliable_setting
                .only_sync_on_change_correction_multiplier,
            rotation_sensitivity: network_behaviour_component
                .network_transform_reliable_setting
                .rotation_sensitivity,
            position_precision: network_behaviour_component
                .network_transform_reliable_setting
                .position_precision,
            scale_precision: network_behaviour_component
                .network_transform_reliable_setting
                .scale_precision,
            compress_rotation: true,
            send_interval_counter: 0,
            last_send_interval_time: f64::MIN,
            last_snapshot: TransformSnapshot::default(),
            last_serialized_position: Default::default(),
            last_deserialized_position: Default::default(),
            last_serialized_scale: Default::default(),
            last_deserialized_scale: Default::default(),
        }
    }

    fn register_delegate()
    where
        Self: Sized,
    {
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

    // OnSerialize()
    fn on_serialize(&mut self, writer: &mut NetworkWriter, initial_state: bool) {
        let mut snapshot = self.construct();
        if initial_state {
            if self.last_snapshot.remote_time > 0.0 {
                snapshot = self.last_snapshot;
            }
            // 写入位置
            if self.sync_position() {
                writer.write_vector3(snapshot.position);
            }
            // 写入旋转
            if self.sync_rotation() {
                if self.compress_rotation {
                    writer.write_uint(snapshot.rotation.compress())
                } else {
                    writer.write_quaternion(snapshot.rotation);
                }
            }
            // 写入缩放
            if self.sync_scale() {
                writer.write_vector3(snapshot.scale);
            }
        } else {
            if self.sync_position() {
                let (_, quantized) = Compress::vector3float_to_vector3long(
                    snapshot.position,
                    self.position_precision,
                );
                DeltaCompression::compress_vector3long(
                    writer,
                    self.last_serialized_position,
                    quantized,
                );
            }
            if self.sync_rotation() {
                if self.compress_rotation {
                    writer.write_uint(snapshot.rotation.compress());
                } else {
                    writer.write_quaternion(snapshot.rotation);
                }
            }
            if self.sync_scale() {
                let (_, quantized) =
                    Compress::vector3float_to_vector3long(snapshot.scale, self.scale_precision);
                DeltaCompression::compress_vector3long(
                    writer,
                    self.last_serialized_scale,
                    quantized,
                );
            }
            // save serialized as 'last' for next delta compression
            if self.sync_position() {
                self.last_serialized_position = Compress::vector3float_to_vector3long(
                    snapshot.position,
                    self.position_precision,
                )
                    .1;
            }
            if self.sync_scale() {
                self.last_serialized_scale =
                    Compress::vector3float_to_vector3long(snapshot.scale, self.scale_precision).1;
            }
            // set 'last'
            self.last_snapshot = snapshot;
        }
    }
    // OnDeserialize()
    fn on_deserialize(&mut self, reader: &mut NetworkReader, initial_state: bool) -> bool {
        let mut position = Vector3::identity();
        let mut rotation = Quaternion::<f32>::identity();
        let mut scale = Vector3::identity();
        if initial_state {
            if self.sync_position() {
                position = reader.read_vector3();
            }
            if self.sync_rotation() {
                if self.compress_rotation {
                    let compressed = reader.read_uint();
                    let decompressed = Quaternion::decompress(compressed);
                    rotation = decompressed;
                } else {
                    rotation = reader.read_quaternion();
                }
            }
            if self.sync_scale() {
                scale = reader.read_vector3();
            }
        } else {
            if self.sync_position() {
                let quantized = DeltaCompression::decompress_vector3long(
                    reader,
                    self.last_deserialized_position,
                );
                position =
                    Compress::vector3long_to_vector3float(quantized, self.position_precision);
            }
            if self.sync_rotation() {
                if self.compress_rotation {
                    let compressed = reader.read_uint();
                    rotation = Quaternion::decompress(compressed);
                } else {
                    rotation = reader.read_quaternion();
                }
            }
            if self.sync_scale() {
                let quantized =
                    DeltaCompression::decompress_vector3long(reader, self.last_deserialized_scale);
                scale = Compress::vector3long_to_vector3float(quantized, self.scale_precision);
            }
        }

        self.on_client_to_server_sync(position, rotation, scale);

        if self.sync_position() {
            (_, self.last_deserialized_position) =
                Compress::vector3float_to_vector3long(position, self.position_precision);
        }
        if self.sync_scale() {
            (_, self.last_deserialized_scale) =
                Compress::vector3float_to_vector3long(scale, self.scale_precision);
        }
        true
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn update(&mut self) {
        self.update_server();
    }

    fn late_update(&mut self) {
        if self.send_interval_counter == self.network_transform_base.send_interval_multiplier
            && (!self.network_transform_base.only_sync_on_change || self.changed(self.construct()))
        {
            self.set_dirty()
        }
        self.u_check_last_send_time();
    }

    fn serialize_sync_vars(&mut self, _writer: &mut NetworkWriter, _initial_state: bool) {}

    fn deserialize_sync_vars(&mut self, _reader: &mut NetworkReader, _initial_state: bool) -> bool {
        true
    }
}

impl NetworkTransformBaseTrait for NetworkTransformReliable {
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
        self.last_deserialized_position = Default::default();
        self.last_deserialized_scale = Default::default();
        self.last_serialized_position = Default::default();
        self.last_serialized_scale = Default::default();
        self.last_snapshot = TransformSnapshot::default();
    }
}
