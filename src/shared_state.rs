use std::any::Any;

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
    pub wm_info_provider: Box<dyn WmInfoProvider>,
}

impl SharedState {
    fn downcast_provider<T: Any>(&mut self) -> Option<&mut T> {
        <dyn Any>::downcast_mut(self.wm_info_provider.as_mut())
    }

    #[cfg(feature = "river")]
    pub fn get_river(&mut self) -> Option<&mut wm_info_provider::RiverInfoProvider> {
        self.downcast_provider()
    }

    #[cfg(feature = "hyprland")]
    pub fn get_hyprland(&mut self) -> Option<&mut wm_info_provider::HyprlandInfoProvider> {
        self.downcast_provider()
    }

    #[cfg(feature = "niri")]
    pub fn get_niri(&mut self) -> Option<&mut wm_info_provider::NiriInfoProvider> {
        self.downcast_provider()
    }
}
