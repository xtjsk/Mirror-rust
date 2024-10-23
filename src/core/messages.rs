use crate::core::batcher::{Batch, DataReader, DataWriter, UnBatch};
use crate::tools::stable_hash::StableHash;
use bytes::Bytes;
use nalgebra::{Quaternion, Vector3};
use std::io;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct TimeSnapshotMessage {}
impl TimeSnapshotMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.TimeSnapshotMessage";
}
impl DataReader<TimeSnapshotMessage> for TimeSnapshotMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<TimeSnapshotMessage> {
        let _ = reader;
        Ok(TimeSnapshotMessage {})
    }
}
impl DataWriter for TimeSnapshotMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(2);
        // 57097
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct ReadyMessage {}
impl ReadyMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.ReadyMessage";
}
impl DataReader<ReadyMessage> for ReadyMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let _ = reader;
        Ok(ReadyMessage {})
    }
}
impl DataWriter for ReadyMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(2);
        // 43708
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct NotReadyMessage {}
impl NotReadyMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.NotReadyMessage";
}
impl DataReader<NotReadyMessage> for NotReadyMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let _ = reader;
        Ok(NotReadyMessage {})
    }
}
impl DataWriter for NotReadyMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(2);
        // 43378
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct AddPlayerMessage {}
impl AddPlayerMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.AddPlayerMessage";
}
impl DataReader<AddPlayerMessage> for AddPlayerMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let _ = reader;
        Ok(AddPlayerMessage {})
    }
}
impl DataWriter for AddPlayerMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(2);
        // 49414
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum SceneOperation {
    Normal = 0,
    LoadAdditive = 1,
    UnloadAdditive = 2,
}
impl SceneOperation {
    pub fn from(value: u8) -> SceneOperation {
        match value {
            0 => SceneOperation::Normal,
            1 => SceneOperation::LoadAdditive,
            2 => SceneOperation::UnloadAdditive,
            _ => SceneOperation::Normal,
        }
    }
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }
}
#[derive(Debug, PartialEq, Clone)]
pub struct SceneMessage {
    pub scene_name: String,
    pub operation: SceneOperation,
    pub custom_handling: bool,
}
impl SceneMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.SceneMessage";
    #[allow(dead_code)]
    pub fn new(
        scene_name: String,
        operation: SceneOperation,
        custom_handling: bool,
    ) -> SceneMessage {
        SceneMessage {
            scene_name,
            operation,
            custom_handling,
        }
    }
}
impl DataReader<SceneMessage> for SceneMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let scene_name = reader.read_string_le()?;
        let operation = SceneOperation::from(reader.read_u8()?);
        let custom_handling = reader.read_bool()?;
        Ok(SceneMessage {
            scene_name,
            operation,
            custom_handling,
        })
    }
}
impl DataWriter for SceneMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        let str_bytes = self.scene_name.as_bytes();
        let total_len = 6 + str_bytes.len() as u64;
        writer.compress_var_u64_le(total_len);
        // 3552
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_string_le(self.scene_name.as_str());
        writer.write_u8(self.operation.to_u8());
        writer.write_bool(self.custom_handling);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CommandMessage {
    pub net_id: u32,
    pub component_index: u8,
    pub function_hash: u16,
    pub payload: Bytes,
}
impl CommandMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.CommandMessage";
    #[allow(dead_code)]
    pub fn new(
        net_id: u32,
        component_index: u8,
        function_hash: u16,
        payload: Bytes,
    ) -> CommandMessage {
        CommandMessage {
            net_id,
            component_index,
            function_hash,
            payload,
        }
    }

    pub fn get_payload(&self) -> Bytes {
        self.payload.clone()
    }
}
impl DataReader<CommandMessage> for CommandMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<CommandMessage> {
        let net_id = reader.read_u32_le()?;
        let component_index = reader.read_u8()?;
        let function_hash = reader.read_u16_le()?;
        let payload = reader.read_remaining()?;
        Ok(CommandMessage {
            net_id,
            component_index,
            function_hash,
            payload,
        })
    }
}
impl DataWriter for CommandMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        // 2 + 4 + 1 + 2 + 4 + self.payload.len()
        let total_len = 13 + self.payload.len() as u64;
        writer.compress_var_u64_le(total_len);
        // 39124
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_u32_le(self.net_id);
        writer.write_u8(self.component_index);
        writer.write_u16_le(self.function_hash);
        writer.write_u32_be(1 + self.payload.len() as u32);
        writer.write(self.payload.as_ref());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct RpcMessage {
    pub net_id: u32,
    pub component_index: u8,
    pub function_hash: u16,
    pub payload: Bytes,
}
impl RpcMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.RpcMessage";
    #[allow(dead_code)]
    pub fn new(net_id: u32, component_index: u8, function_hash: u16, payload: Bytes) -> RpcMessage {
        RpcMessage {
            net_id,
            component_index,
            function_hash,
            payload,
        }
    }

    #[allow(dead_code)]
    pub fn get_payload_no_len(&self) -> Bytes {
        self.payload.slice(4..)
    }
}
impl DataReader<RpcMessage> for RpcMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let net_id = reader.read_u32_le()?;
        let component_index = reader.read_u8()?;
        let function_hash = reader.read_u16_le()?;
        let payload = reader.read_remaining()?;
        Ok(RpcMessage {
            net_id,
            component_index,
            function_hash,
            payload,
        })
    }
}
impl DataWriter for RpcMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        // 2 + 4 + 1 + 2 + 4 + self.payload.len()
        let total_len = 13 + self.payload.len() as u64;
        writer.compress_var_u64_le(total_len);
        // 40238
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_u32_le(self.net_id);
        writer.write_u8(self.component_index);
        writer.write_u16_le(self.function_hash);
        writer.write_u32_le(1 + self.payload.len() as u32);
        writer.write(self.payload.as_ref());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct SpawnMessage {
    pub net_id: u32,
    pub is_local_player: bool,
    pub is_owner: bool,
    pub scene_id: u64,
    pub asset_id: u32,
    pub position: Vector3<f32>,
    pub rotation: Quaternion<f32>,
    pub scale: Vector3<f32>,
    pub payload: Bytes,
}
impl SpawnMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.SpawnMessage";
    #[allow(dead_code)]
    pub fn new(
        net_id: u32,
        is_local_player: bool,
        is_owner: bool,
        scene_id: u64,
        asset_id: u32,
        position: Vector3<f32>,
        rotation: Quaternion<f32>,
        scale: Vector3<f32>,
        payload: Bytes,
    ) -> SpawnMessage {
        SpawnMessage {
            net_id,
            is_local_player,
            is_owner,
            scene_id,
            asset_id,
            position,
            rotation,
            scale,
            payload,
        }
    }
    #[allow(dead_code)]
    pub fn get_payload(&self) -> Bytes {
        self.payload.clone()
    }
}
impl DataReader<SpawnMessage> for SpawnMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let net_id = reader.read_u32_le()?;
        let is_local_player = reader.read_bool()?;
        let is_owner = reader.read_bool()?;
        let scene_id = reader.read_u64_le()?;
        let asset_id = reader.read_u32_le()?;
        let position = reader.read_vector3_f32_le()?;
        let rotation = reader.read_quaternion_f32_le()?;
        let scale = reader.read_vector3_f32_le()?;
        let payload = reader.read_remaining()?;
        Ok(SpawnMessage {
            net_id,
            is_local_player,
            is_owner,
            scene_id,
            asset_id,
            position,
            rotation,
            scale,
            payload,
        })
    }
}

impl DataWriter for SpawnMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        // 2 + 4 + 1 + 1 + 8 + 12 * 4 + self.payload.len()
        let total_len = 64 + self.payload.len() as u64;
        writer.compress_var_u64_le(total_len);
        // 12504
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_u32_le(self.net_id);
        writer.write_bool(self.is_local_player);
        writer.write_bool(self.is_owner);
        writer.write_u64_le(self.scene_id);
        writer.write_u32_le(self.asset_id);
        writer.write_vector3_f32_le(self.position);
        writer.write_quaternion_f32_le(self.rotation);
        writer.write_vector3_f32_le(self.scale);
        writer.write_u32_le(1 + self.payload.len() as u32);
        writer.write(self.payload.as_ref());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ChangeOwnerMessage {
    pub net_id: u32,
    pub is_owner: bool,
    pub is_local_player: bool,
}
impl ChangeOwnerMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.ChangeOwnerMessage";
    #[allow(dead_code)]
    pub fn new(net_id: u32, is_owner: bool, is_local_player: bool) -> Self {
        Self {
            net_id,
            is_owner,
            is_local_player,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ObjectSpawnStartedMessage {}
impl ObjectSpawnStartedMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.ObjectSpawnStartedMessage";
}
impl DataReader<ObjectSpawnStartedMessage> for ObjectSpawnStartedMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let _ = reader;
        Ok(ObjectSpawnStartedMessage {})
    }
}
impl DataWriter for ObjectSpawnStartedMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(2);
        // 12504
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ObjectSpawnFinishedMessage {}
impl ObjectSpawnFinishedMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.ObjectSpawnFinishedMessage";
}
impl DataReader<ObjectSpawnFinishedMessage> for ObjectSpawnFinishedMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let _ = reader;
        Ok(ObjectSpawnFinishedMessage {})
    }
}
impl DataWriter for ObjectSpawnFinishedMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(2);
        // 43444
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ObjectDestroyMessage {
    pub net_id: u32,
}
impl ObjectDestroyMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.ObjectDestroyMessage";
    #[allow(dead_code)]
    pub fn new(net_id: u32) -> ObjectDestroyMessage {
        ObjectDestroyMessage { net_id }
    }
}
impl DataReader<ObjectDestroyMessage> for ObjectDestroyMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let net_id = reader.read_u32_le()?;
        Ok(ObjectDestroyMessage { net_id })
    }
}
impl DataWriter for ObjectDestroyMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(6);
        // 12504
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_u32_le(self.net_id);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ObjectHideMessage {
    pub net_id: u32,
}
impl ObjectHideMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.ObjectHideMessage";
    #[allow(dead_code)]
    pub fn new(net_id: u32) -> Self {
        Self { net_id }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct EntityStateMessage {
    pub net_id: u32,
    pub payload: Bytes,
}
impl EntityStateMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.EntityStateMessage";
    #[allow(dead_code)]
    pub fn new(net_id: u32, payload: Bytes) -> EntityStateMessage {
        EntityStateMessage { net_id, payload }
    }

    #[allow(dead_code)]
    pub fn get_payload_no_len(&self) -> Vec<u8> {
        self.payload[4..].to_vec()
    }
}
impl DataReader<EntityStateMessage> for EntityStateMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let net_id = reader.read_u32_le()?;
        let payload = reader.read_remaining()?;
        Ok(EntityStateMessage { net_id, payload })
    }
}
impl DataWriter for EntityStateMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        // 2 + 4 + 4 + self.payload.len()
        let total_len = 10 + self.payload.len() as u64;
        writer.compress_var_u64_le(total_len);
        // 12504
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_u32_le(self.net_id);
        writer.write_u32_le(1 + self.payload.len() as u32);
        writer.write(self.payload.as_ref());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct NetworkPingMessage {
    pub local_time: f64,
    pub predicted_time_adjusted: f64,
}
impl NetworkPingMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.NetworkPingMessage";
    #[allow(dead_code)]
    pub fn new(local_time: f64, predicted_time_adjusted: f64) -> Self {
        Self {
            local_time,
            predicted_time_adjusted,
        }
    }
}
impl DataReader<NetworkPingMessage> for NetworkPingMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let local_time = reader.read_f64_le()?;
        let predicted_time_adjusted = reader.read_f64_le()?;
        Ok(NetworkPingMessage {
            local_time,
            predicted_time_adjusted,
        })
    }
}
impl DataWriter for NetworkPingMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(18);
        // 17487
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_f64_le(self.local_time);
        writer.write_f64_le(self.predicted_time_adjusted);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct NetworkPongMessage {
    pub local_time: f64,
    pub prediction_error_unadjusted: f64,
    pub prediction_error_adjusted: f64,
}
impl NetworkPongMessage {
    #[allow(dead_code)]
    pub const FULL_NAME: &'static str = "Mirror.NetworkPongMessage";
    #[allow(dead_code)]
    pub fn new(
        local_time: f64,
        prediction_error_unadjusted: f64,
        prediction_error_adjusted: f64,
    ) -> NetworkPongMessage {
        NetworkPongMessage {
            local_time,
            prediction_error_unadjusted,
            prediction_error_adjusted,
        }
    }
}
impl DataReader<NetworkPongMessage> for NetworkPongMessage {
    fn deserialize(reader: &mut UnBatch) -> io::Result<Self> {
        let local_time = reader.read_f64_le()?;
        let prediction_error_unadjusted = reader.read_f64_le()?;
        let prediction_error_adjusted = reader.read_f64_le()?;
        Ok(NetworkPongMessage {
            local_time,
            prediction_error_unadjusted,
            prediction_error_adjusted,
        })
    }
}
impl DataWriter for NetworkPongMessage {
    fn serialize(&mut self, writer: &mut Batch) {
        writer.compress_var_u64_le(26);
        // 27095
        writer.write_u16_le(Self::FULL_NAME.get_stable_hash_code16());
        writer.write_f64_le(self.local_time);
        writer.write_f64_le(self.prediction_error_unadjusted);
        writer.write_f64_le(self.prediction_error_adjusted);
    }
}