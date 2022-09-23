pub mod protocol {
    use smithay_client_toolkit::reexports::client as wayland_client;
    use smithay_client_toolkit::reexports::client::protocol::*;

    pub mod __interfaces {
        use smithay_client_toolkit::reexports::client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/river-status-unstable-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/river-status-unstable-v1.xml");
}

mod dispatch;

use std::sync::{Arc, Weak};

use smithay_client_toolkit::globals::GlobalData;
use wayland_client::{protocol::wl_output, Connection, Dispatch, QueueHandle};

use protocol::{zriver_output_status_v1, zriver_status_manager_v1};

use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::registry::GlobalProxy;

#[derive(Debug)]
pub struct RiverStatusState {
    status_manager: GlobalProxy<zriver_status_manager_v1::ZriverStatusManagerV1>,
    output_statuses: Vec<Weak<OutputStatusInner>>,
}

impl RiverStatusState {
    pub fn new() -> Self {
        Self {
            status_manager: GlobalProxy::NotReady,
            output_statuses: Vec::new(),
        }
    }

    pub fn status_manager(
        &self,
    ) -> Result<&zriver_status_manager_v1::ZriverStatusManagerV1, GlobalError> {
        self.status_manager.get()
    }

    pub fn get_output_status(
        &self,
        status: &zriver_output_status_v1::ZriverOutputStatusV1,
    ) -> Option<RiverOutputStatus> {
        self.output_statuses
            .iter()
            .filter_map(Weak::upgrade)
            .find(|inner| &inner.status == status)
            .map(RiverOutputStatus)
    }

    #[must_use = "The output status is destroyed if dropped"]
    pub fn new_output_status<D>(
        &mut self,
        qh: &QueueHandle<D>,
        output: &wl_output::WlOutput,
    ) -> Result<RiverOutputStatus, GlobalError>
    where
        D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, GlobalData>
            + Dispatch<zriver_output_status_v1::ZriverOutputStatusV1, RiverStatusData>
            + 'static,
    {
        let manager = self.status_manager()?;
        let output_status = manager.get_river_output_status(output, qh, RiverStatusData {});

        let output_status = RiverOutputStatus(Arc::new(OutputStatusInner {
            status: output_status,
        }));

        self.output_statuses.push(Arc::downgrade(&output_status.0));

        Ok(output_status)
    }
}

pub trait RiverStatusHandler: Sized {
    fn river_status_state(&mut self) -> &mut RiverStatusState;

    fn focused_tags_updated(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        focused: u32,
    );

    fn urgent_tags_updated(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        urgent: u32,
    );

    fn views_tags_updated(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        tags: Vec<u32>,
    );
}

#[derive(Debug, Clone)]
pub struct RiverOutputStatus(Arc<OutputStatusInner>);

impl PartialEq for RiverOutputStatus {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug)]
pub struct RiverStatusData {
    // This is empty right now, but may be populated in the future.
}

#[macro_export]
macro_rules! delegate_river_status {
    ($ty: ty) => {
        ::smithay_client_toolkit::reexports::client::delegate_dispatch!($ty: [
            $crate::river_protocols::status::protocol::zriver_status_manager_v1::ZriverStatusManagerV1: ::smithay_client_toolkit::globals::GlobalData
        ] => $crate::river_protocols::status::RiverStatusState);
        ::smithay_client_toolkit::reexports::client::delegate_dispatch!($ty: [
            $crate::river_protocols::status::protocol::zriver_output_status_v1::ZriverOutputStatusV1: $crate::river_protocols::status::RiverStatusData
        ] => $crate::river_protocols::status::RiverStatusState);
    };
}

#[derive(Debug)]
struct OutputStatusInner {
    status: zriver_output_status_v1::ZriverOutputStatusV1,
}

impl Drop for OutputStatusInner {
    fn drop(&mut self) {
        self.status.destroy();
    }
}
