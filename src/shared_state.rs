use smithay_client_toolkit::{reexports::client::QueueHandle, shm::slot::SlotPool};

use crate::{
    config::Config,
    i3bar_protocol::Block,
    state::{ComputedBlock, State},
    status_cmd::StatusCmd,
};

#[derive(Debug)]
pub struct SharedState {
    pub qh: QueueHandle<State>,
    pub pool: SlotPool,
    pub config: Config,
    pub status_cmd: Option<StatusCmd>,
    pub blocks: Vec<Block>,
    pub blocks_cache: Vec<ComputedBlock>,
}
