use crate::{
    blocks_cache::BlocksCache, config::Config, status_cmd::StatusCmd,
    wm_info_provider::WmInfoProvider,
};

use wayrs_utils::shm_alloc::ShmAlloc;

pub struct SharedState {
    pub shm: ShmAlloc,
    pub config: Config,
    pub status_cmd: Option<StatusCmd>,
    pub blocks_cache: BlocksCache,
    pub wm_info_provider: WmInfoProvider,
}
