use crate::components::network_behaviour::{NetworkBehaviour, NetworkBehaviourTrait};
use crate::components::SyncVar;
use crate::core::backend_data::NetworkBehaviourSetting;
use crate::core::batcher::{Batch, UnBatch};
use dashmap::DashMap;
use std::any::Any;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct NetworkCommonComponent {
    pub network_behaviour: NetworkBehaviour,
    pub sync_vars: DashMap<u8, SyncVar>,
}

impl NetworkCommonComponent {
    #[allow(dead_code)]
    pub const COMPONENT_TAG: &'static str = "Mirror.NetworkCommon";
    pub fn new(network_behaviour_setting: NetworkBehaviourSetting, component_index: u8, sync_vars: DashMap<u8, SyncVar>) -> Self {
        NetworkCommonComponent {
            network_behaviour: NetworkBehaviour::new(network_behaviour_setting, component_index),
            sync_vars,
        }
    }
}

impl NetworkBehaviourTrait for NetworkCommonComponent {
    fn deserialize_objects_all(&self, un_batch: UnBatch, initial_state: bool) {}

    fn serialize(&self, initial_state: bool) -> Batch {
        let mut batch = Batch::new();
        for i in 0..self.sync_vars.len() as u8 {
            if let Some(sync_var) = self.sync_vars.get(&i) {
                batch.write(sync_var.data.as_ref());
            }
        }
        batch
    }

    fn deserialize(&self, un_batch: &mut UnBatch, initial_state: bool) {}

    fn get_network_behaviour(&self) -> &NetworkBehaviour {
        &self.network_behaviour
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}