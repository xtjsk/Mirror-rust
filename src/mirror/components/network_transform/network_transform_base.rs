use crate::mirror::components::network_transform::transform_snapshot::TransformSnapshot;
use crate::mirror::core::backend_data::{NetworkBehaviourSetting, NetworkTransformBaseSetting};
use crate::mirror::core::network_behaviour::{GameObject, NetworkBehaviour};
use crate::mirror::core::network_manager::NetworkManagerStatic;
use crate::mirror::core::network_server::NetworkServerStatic;
use crate::mirror::core::network_time::NetworkTime;
use crate::mirror::core::snapshot_interpolation::snapshot_interpolation::SnapshotInterpolation;
use nalgebra::{Quaternion, Vector3};
use ordered_float::OrderedFloat;
use std::collections::BTreeMap;

#[derive(Debug, PartialOrd, PartialEq)]
pub enum CoordinateSpace {
    Local,
    World,
}

impl CoordinateSpace {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => CoordinateSpace::Local,
            1 => CoordinateSpace::World,
            _ => CoordinateSpace::Local,
        }
    }
}

#[derive(Debug)]
pub struct NetworkTransformBase {
    pub network_behaviour: NetworkBehaviour,
    pub coordinate_space: CoordinateSpace,
    pub is_client_with_authority: bool,
    pub server_snapshots: BTreeMap<OrderedFloat<f64>, TransformSnapshot>,
    pub time_stamp_adjustment: f64,
    pub offset: f64,
    // pub network_behaviour_setting: NetworkBehaviourSetting,
    pub sync_position: bool,
    pub sync_rotation: bool,
    pub sync_scale: bool,
    pub only_sync_on_change: bool,
    pub compress_rotation: bool,
    pub interpolate_position: bool,
    pub interpolate_rotation: bool,
    pub interpolate_scale: bool,
    pub send_interval_multiplier: u32,
    pub timeline_offset: bool,
}

impl NetworkTransformBase {
    pub fn new(game_object: GameObject, network_transform_base_setting: NetworkTransformBaseSetting, network_behaviour_setting: NetworkBehaviourSetting, component_index: u8, sub_class: String) -> Self {
        let mut base = Self {
            network_behaviour: NetworkBehaviour::new(game_object, network_behaviour_setting, component_index, sub_class),
            is_client_with_authority: false,
            server_snapshots: Default::default(),
            time_stamp_adjustment: 0.0,
            offset: 0.0,
            // network_behaviour_setting: NetworkBehaviourSetting::new(network_behaviour_setting),
            sync_position: network_transform_base_setting.sync_position,
            sync_rotation: network_transform_base_setting.sync_rotation,
            sync_scale: network_transform_base_setting.sync_scale,
            only_sync_on_change: network_transform_base_setting.only_sync_on_change,
            compress_rotation: network_transform_base_setting.compress_rotation,
            interpolate_position: network_transform_base_setting.interpolate_position,
            interpolate_rotation: network_transform_base_setting.interpolate_rotation,
            interpolate_scale: network_transform_base_setting.interpolate_scale,
            coordinate_space: CoordinateSpace::from_u8(network_transform_base_setting.coordinate_space),
            send_interval_multiplier: network_transform_base_setting.send_interval_multiplier,
            timeline_offset: network_transform_base_setting.timeline_offset,
        };
        base.time_stamp_adjustment = NetworkServerStatic::send_interval() as f64 * (base.send_interval_multiplier as f64 - 1.0);
        if base.timeline_offset {
            base.offset = NetworkServerStatic::send_interval() as f64 * base.send_interval_multiplier as f64;
        }
        base
    }
    pub fn reset_state(&mut self) {
        self.server_snapshots.clear();
    }
}

pub trait NetworkTransformBaseTrait {
    fn coordinate_space(&self) -> &CoordinateSpace;
    fn set_coordinate_space(&mut self, value: CoordinateSpace);
    fn get_game_object(&self) -> &GameObject;
    fn set_game_object(&mut self, value: GameObject);
    fn get_position(&self) -> Vector3<f32> {
        if self.coordinate_space() == &CoordinateSpace::Local {
            self.get_game_object().transform.local_position
        } else {
            self.get_game_object().transform.position
        }
    }
    fn set_position(&mut self, value: Vector3<f32>) {
        let mut game_object = self.get_game_object().clone();
        if *self.coordinate_space() == CoordinateSpace::Local {
            game_object.transform.local_position = value;
        } else {
            game_object.transform.position = value;
        }
        self.set_game_object(game_object);
    }
    fn get_rotation(&self) -> Quaternion<f32> {
        if *self.coordinate_space() == CoordinateSpace::Local {
            self.get_game_object().transform.local_rotation
        } else {
            self.get_game_object().transform.rotation
        }
    }
    fn set_rotation(&mut self, value: Quaternion<f32>) {
        let mut game_object = self.get_game_object().clone();
        if self.coordinate_space() == &CoordinateSpace::Local {
            game_object.transform.local_rotation = value;
        } else {
            game_object.transform.rotation = value;
        }
        self.set_game_object(game_object);
    }
    fn get_scale(&self) -> Vector3<f32> {
        if self.coordinate_space() == &CoordinateSpace::Local {
            self.get_game_object().transform.local_scale
        } else {
            self.get_game_object().transform.scale
        }
    }
    fn set_scale(&mut self, value: Vector3<f32>) {
        let mut game_object = self.get_game_object().clone();
        if self.coordinate_space() == &CoordinateSpace::Local {
            game_object.transform.local_scale = value;
        } else {
            game_object.transform.scale = value;
        }
        self.set_game_object(game_object);
    }
    // Construct()
    fn construct(&self) -> TransformSnapshot {
        TransformSnapshot {
            position: self.get_position(),
            rotation: self.get_rotation(),
            scale: self.get_scale(),
            remote_time: NetworkTime::local_time(),
            local_time: 0.0,
        }
    }
    fn sync_position(&self) -> bool;
    fn sync_rotation(&self) -> bool;
    fn interpolate_position(&self) -> bool;
    fn interpolate_rotation(&self) -> bool;
    fn interpolate_scale(&self) -> bool;
    fn sync_scale(&self) -> bool;
    fn reset_state(&mut self);
    // void AddSnapshot
    fn add_snapshot(&self, snapshots: &mut BTreeMap<OrderedFloat<f64>, TransformSnapshot>, timestamp: f64, mut position: Option<Vector3<f32>>, mut rotation: Option<Quaternion<f32>>, mut scale: Option<Vector3<f32>>) {
        let last_snapshot = snapshots.iter().last();
        if position.is_none() {
            if let Some((_, last_snapshot)) = last_snapshot {
                position = Some(last_snapshot.position);
            } else {
                position = Some(self.get_position());
            }
        }
        if rotation.is_none() {
            if let Some((_, last_snapshot)) = last_snapshot {
                rotation = Some(last_snapshot.rotation);
            } else {
                rotation = Some(self.get_rotation());
            }
        }
        if scale.is_none() {
            if let Some((_, last_snapshot)) = last_snapshot {
                scale = Some(last_snapshot.scale);
            } else {
                scale = Some(self.get_scale());
            }
        }
        let snapshot = TransformSnapshot::new(timestamp,
                                              NetworkTime::local_time(),
                                              position.unwrap(),
                                              rotation.unwrap(),
                                              scale.unwrap());
        let snapshot_settings = &NetworkManagerStatic::network_manager_singleton().snapshot_interpolation_settings();
        SnapshotInterpolation::insert_if_not_exists(snapshots,
                                                    snapshot_settings.buffer_limit,
                                                    snapshot);
    }
    // Apply
    fn apply(&mut self, interpolated: TransformSnapshot, end_goal: TransformSnapshot) {
        if self.sync_position() {
            if self.interpolate_position() {
                self.set_position(interpolated.position);
            } else {
                self.set_position(end_goal.position);
            }
        }
        if self.sync_rotation() {
            if self.interpolate_rotation() {
                self.set_rotation(interpolated.rotation);
            } else {
                self.set_rotation(end_goal.rotation);
            }
        }
        if self.sync_scale() {
            if self.interpolate_scale() {
                self.set_scale(interpolated.scale);
            } else {
                self.set_scale(end_goal.scale);
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Transform {
    pub position: Vector3<f32>,
    pub rotation: Quaternion<f32>,
    pub scale: Vector3<f32>,

    pub local_position: Vector3<f32>,
    pub local_rotation: Quaternion<f32>,
    pub local_scale: Vector3<f32>,
}

// GameObject 的 Transform 组件
impl Transform {
    pub fn new(position: Vector3<f32>,
               rotation: Quaternion<f32>,
               scale: Vector3<f32>,
               local_position: Vector3<f32>,
               local_rotation: Quaternion<f32>,
               local_scale: Vector3<f32>) -> Self {
        Self {
            position,
            rotation,
            scale,
            local_position,
            local_rotation,
            local_scale,
        }
    }

    pub fn default() -> Self {
        Self::new(Vector3::new(0.0, 0.5, 0.0),
                  Quaternion::new(1.0, 0.0, 0.0, 0.0),
                  Vector3::new(1.0, 1.0, 1.0),
                  Vector3::new(0.0, 0.5, 0.0),
                  Quaternion::new(1.0, 0.0, 0.0, 0.0),
                  Vector3::new(1.0, 1.0, 1.0))
    }
}