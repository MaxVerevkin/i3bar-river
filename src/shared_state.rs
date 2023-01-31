use crate::{
    config::Config, i3bar_protocol::Block, state::ComputedBlock, status_cmd::StatusCmd,
    wm_info_provider::WmInfoProvider,
};
use wayrs_shm_alloc::ShmAlloc;

pub struct SharedState {
    pub shm: ShmAlloc,
    pub config: Config,
    pub status_cmd: Option<StatusCmd>,
    pub blocks: Vec<Block>,
    pub blocks_cache: Vec<ComputedBlock>,
    pub wm_info_provider: Option<Box<dyn WmInfoProvider>>,
}
