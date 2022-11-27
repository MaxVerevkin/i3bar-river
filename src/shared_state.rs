use smithay_client_toolkit::{
    reexports::client::QueueHandle,
    shm::{slot::SlotPool, ShmState},
};

use crate::{
    config::Config,
    i3bar_protocol::Block,
    state::{ComputedBlock, State},
    status_cmd::StatusCmd,
};

#[derive(Debug)]
pub struct SharedState {
    pub qh: QueueHandle<State>,
    pub shm_state: ShmState,
    pub pool: Option<SlotPool>,
    pub config: Config,
    pub status_cmd: Option<StatusCmd>,
    pub blocks: Vec<Block>,
    pub blocks_cache: Vec<ComputedBlock>,
}

impl SharedState {
    pub fn get_pool(&mut self, len: usize) -> &mut SlotPool {
        self.pool.get_or_insert_with(|| {
            SlotPool::new(len, &self.shm_state).expect("Failed to create pool")
        })
    }
}
