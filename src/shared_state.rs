use crate::{
    blocks_cache::BlocksCache,
    config::Config,
    status_cmd::StatusCmd,
    wm_info_provider::{self, WmInfoProvider},
};

use wayrs_utils::shm_alloc::ShmAlloc;

pub struct SharedState {
    pub shm: ShmAlloc,
    pub config: Config,
    pub status_cmd: Option<StatusCmd>,
    pub blocks_cache: BlocksCache,
    pub wm_info_provider: Option<Box<dyn WmInfoProvider>>,
}

impl SharedState {
    pub fn get_river(&mut self) -> Option<&mut wm_info_provider::RiverInfoProvider> {
        self.wm_info_provider.as_mut()?.as_any().downcast_mut()
    }

    pub fn get_hyprland(&mut self) -> Option<&mut wm_info_provider::Hyprland> {
        self.wm_info_provider.as_mut()?.as_any().downcast_mut()
    }
}
