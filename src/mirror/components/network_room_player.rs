use crate::log_error;
use crate::mirror::core::backend_data::NetworkBehaviourComponent;
use crate::mirror::core::network_behaviour::{
    GameObject, NetworkBehaviour, NetworkBehaviourTrait, SyncDirection, SyncMode,
};
use crate::mirror::core::network_identity::NetworkIdentity;
use crate::mirror::core::network_manager::NetworkManagerStatic;
use crate::mirror::core::network_reader::{NetworkReader, NetworkReaderTrait};
use crate::mirror::core::network_server::NetworkServerStatic;
use crate::mirror::core::network_writer::{NetworkWriter, NetworkWriterTrait};
use crate::mirror::core::remote_calls::RemoteProcedureCalls;
use crate::mirror::core::sync_object::SyncObject;
use std::any::Any;
use std::sync::Once;

#[derive(Debug)]
pub struct NetworkRoomPlayer {
    pub network_behaviour: NetworkBehaviour,
    pub ready_to_begin: bool,
    pub index: i32,
}

impl NetworkRoomPlayer {
    fn invoke_user_code_cmd_change_ready_state_boolean(
        identity: &mut NetworkIdentity,
        component_index: u8,
        _func_hash: u16,
        reader: &mut NetworkReader,
        _conn_id: u64,
    ) {
        if !NetworkServerStatic::active() {
            log_error!("Command CmdClientToServerSync called on client.");
            return;
        }
        NetworkBehaviour::early_invoke(identity, component_index)
            .as_any_mut()
            .downcast_mut::<Self>()
            .unwrap()
            .user_code_cmd_change_ready_state_boolean(reader.read_bool());
        NetworkBehaviour::late_invoke(identity, component_index);
    }

    fn user_code_cmd_change_ready_state_boolean(&mut self, value: bool) {
        self.ready_to_begin = value;
        NetworkManagerStatic::network_manager_singleton().ready_status_changed();
    }
}

impl NetworkBehaviourTrait for NetworkRoomPlayer {
    fn new(game_object: GameObject, network_behaviour_component: &NetworkBehaviourComponent) -> Self
    where
        Self: Sized,
    {
        Self::register_delegate();
        Self {
            network_behaviour: NetworkBehaviour::new(
                game_object,
                network_behaviour_component.network_behaviour_setting,
                network_behaviour_component.index,
                network_behaviour_component.sub_class.clone(),
            ),
            ready_to_begin: false,
            index: 0,
        }
    }

    fn register_delegate()
    where
        Self: Sized,
    {
        // RemoteProcedureCalls.RegisterCommand(typeof (NetworkRoomPlayer), "System.Void Mirror.NetworkRoomPlayer::CmdChangeReadyState(System.Boolean)", new RemoteCallDelegate(NetworkRoomPlayer.InvokeUserCode_CmdChangeReadyState__Boolean), true);
        RemoteProcedureCalls::register_command_delegate::<Self>(
            "System.Void Mirror.NetworkRoomPlayer::CmdChangeReadyState(System.Boolean)",
            Self::invoke_user_code_cmd_change_ready_state_boolean,
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
        self.network_behaviour.sync_interval
    }

    fn set_sync_interval(&mut self, value: f64) {
        self.network_behaviour.sync_interval = value;
    }

    fn last_sync_time(&self) -> f64 {
        self.network_behaviour.last_sync_time
    }

    fn set_last_sync_time(&mut self, value: f64) {
        self.network_behaviour.last_sync_time = value;
    }

    fn sync_direction(&mut self) -> &SyncDirection {
        &self.network_behaviour.sync_direction
    }

    fn set_sync_direction(&mut self, value: SyncDirection) {
        self.network_behaviour.sync_direction = value;
    }

    fn sync_mode(&mut self) -> &SyncMode {
        &self.network_behaviour.sync_mode
    }

    fn set_sync_mode(&mut self, value: SyncMode) {
        self.network_behaviour.sync_mode = value;
    }

    fn index(&self) -> u8 {
        self.network_behaviour.index
    }

    fn set_index(&mut self, value: u8) {
        self.network_behaviour.index = value;
    }

    fn sub_class(&self) -> String {
        self.network_behaviour.sub_class.clone()
    }

    fn set_sub_class(&mut self, value: String) {
        self.network_behaviour.sub_class = value;
    }

    fn sync_var_dirty_bits(&self) -> u64 {
        self.network_behaviour.sync_var_dirty_bits
    }

    fn __set_sync_var_dirty_bits(&mut self, value: u64) {
        self.network_behaviour.sync_var_dirty_bits = value;
    }

    fn sync_object_dirty_bits(&self) -> u64 {
        self.network_behaviour.sync_object_dirty_bits
    }

    fn __set_sync_object_dirty_bits(&mut self, value: u64) {
        self.network_behaviour.sync_object_dirty_bits = value;
    }

    fn net_id(&self) -> u32 {
        self.network_behaviour.net_id
    }

    fn set_net_id(&mut self, value: u32) {
        self.network_behaviour.net_id = value;
    }

    fn connection_to_client(&self) -> u64 {
        self.network_behaviour.connection_to_client
    }

    fn set_connection_to_client(&mut self, value: u64) {
        self.network_behaviour.connection_to_client = value;
    }

    fn observers(&self) -> &Vec<u64> {
        &self.network_behaviour.observers
    }

    fn set_observers(&mut self, value: Vec<u64>) {
        self.network_behaviour.observers = value
    }

    fn game_object(&self) -> &GameObject {
        &self.network_behaviour.game_object
    }

    fn set_game_object(&mut self, value: GameObject) {
        self.network_behaviour.game_object = value
    }

    fn sync_objects(&mut self) -> &mut Vec<Box<dyn SyncObject>> {
        &mut self.network_behaviour.sync_objects
    }

    fn set_sync_objects(&mut self, value: Vec<Box<dyn SyncObject>>) {
        self.network_behaviour.sync_objects = value
    }

    fn add_sync_object(&mut self, value: Box<dyn SyncObject>) {
        self.network_behaviour.sync_objects.push(value);
    }

    fn sync_var_hook_guard(&self) -> u64 {
        self.network_behaviour.sync_var_hook_guard
    }

    fn __set_sync_var_hook_guard(&mut self, value: u64) {
        self.network_behaviour.sync_var_hook_guard = value
    }

    fn is_dirty(&self) -> bool {
        self.network_behaviour.is_dirty()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // 在第一次 update 之前仅调用一次
    fn start(&mut self) {
        static ONCE: Once = Once::new();
        let _ = &ONCE.call_once(|| {
            let network_manager = NetworkManagerStatic::network_manager_singleton();
            if network_manager.dont_destroy_on_load() {}

            network_manager.room_slots().push(self.net_id());

            if NetworkServerStatic::active() {
                network_manager.recalculate_room_player_indices();
            }
        });
    }

    fn serialize_sync_vars(&mut self, writer: &mut NetworkWriter, initial_state: bool) {
        if initial_state {
            writer.write_bool(self.ready_to_begin);
            writer.write_int(self.index);
        } else {
            writer.compress_var_ulong(self.sync_var_dirty_bits());
            if self.sync_var_dirty_bits() & (1 << 0) != 0 {
                writer.write_bool(self.ready_to_begin);
            }
            if self.sync_var_dirty_bits() & (1 << 1) != 0 {
                writer.write_int(self.index);
            }
        }
    }

    fn deserialize_sync_vars(&mut self, _reader: &mut NetworkReader, _initial_state: bool) -> bool {
        true
    }
}
