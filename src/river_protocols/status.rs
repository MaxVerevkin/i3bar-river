use std::sync::{Arc, Weak};

use wayland_client::{
    globals::{BindError, GlobalList},
    protocol::wl_output,
    Connection, Dispatch, QueueHandle,
};

use protocol::{zriver_output_status_v1, zriver_status_manager_v1};

#[derive(Debug, Clone)]
pub struct RiverStatusState {
    status_manager: zriver_status_manager_v1::ZriverStatusManagerV1,
    output_statuses: Vec<Weak<OutputStatusInner>>,
}

impl RiverStatusState {
    pub fn new<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Result<Self, BindError>
    where
        D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, (), D> + 'static,
    {
        let status_manager = globals.bind(qh, 1..=3, ())?;
        Ok(Self {
            status_manager,
            output_statuses: Vec::new(),
        })
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
    ) -> RiverOutputStatus
    where
        D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, ()>
            + Dispatch<zriver_output_status_v1::ZriverOutputStatusV1, RiverStatusData>
            + 'static,
    {
        let output_status =
            self.status_manager
                .get_river_output_status(output, qh, RiverStatusData {});

        let output_status = RiverOutputStatus(Arc::new(OutputStatusInner {
            status: output_status,
        }));

        self.output_statuses.push(Arc::downgrade(&output_status.0));

        output_status
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

    fn layout_name_updated(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        layout_name: Option<String>,
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
            $crate::river_protocols::status::protocol::zriver_status_manager_v1::ZriverStatusManagerV1: ()
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

impl<D> Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, (), D> for RiverStatusState
where
    D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, ()> + RiverStatusHandler + 'static,
{
    fn event(
        _: &mut D,
        _: &zriver_status_manager_v1::ZriverStatusManagerV1,
        _: zriver_status_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("zriver_status_manager_v1 has no events")
    }
}

impl<D> Dispatch<zriver_output_status_v1::ZriverOutputStatusV1, RiverStatusData, D>
    for RiverStatusState
where
    D: Dispatch<zriver_output_status_v1::ZriverOutputStatusV1, RiverStatusData>
        + RiverStatusHandler
        + 'static,
{
    fn event(
        data: &mut D,
        status: &zriver_output_status_v1::ZriverOutputStatusV1,
        event: zriver_output_status_v1::Event,
        _udata: &RiverStatusData,
        conn: &Connection,
        qh: &QueueHandle<D>,
    ) {
        use zriver_output_status_v1::Event;

        // Remove any statuses that have been dropped
        data.river_status_state()
            .output_statuses
            .retain(|status| status.upgrade().is_some());

        if let Some(status) = data.river_status_state().get_output_status(status) {
            match event {
                Event::FocusedTags { tags } => data.focused_tags_updated(conn, qh, &status, tags),
                Event::UrgentTags { tags } => data.urgent_tags_updated(conn, qh, &status, tags),
                Event::ViewTags { tags } => {
                    let tags = tags
                        .chunks_exact(4)
                        .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                        .collect();
                    data.views_tags_updated(conn, qh, &status, tags);
                }
                Event::LayoutName { name } => {
                    data.layout_name_updated(conn, qh, &status, Some(name))
                }
                Event::LayoutNameClear => data.layout_name_updated(conn, qh, &status, None),
            }
        }
    }
}

pub mod protocol {
    #![allow(non_upper_case_globals)]

    use smithay_client_toolkit::reexports::client as wayland_client;
    use smithay_client_toolkit::reexports::client::protocol::*;

    pub mod __interfaces {
        use smithay_client_toolkit::reexports::client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/river-status-unstable-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/river-status-unstable-v1.xml");
}
