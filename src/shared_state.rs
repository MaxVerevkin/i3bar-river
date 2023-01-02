use crate::{config::Config, i3bar_protocol::Block, state::ComputedBlock, status_cmd::StatusCmd};
use wayrs_shm_alloc::ShmAlloc;

pub struct SharedState {
    pub shm: ShmAlloc,
    pub config: Config,
    pub status_cmd: Option<StatusCmd>,
    pub blocks: Vec<Block>,
    pub blocks_cache: Vec<ComputedBlock>,
}
