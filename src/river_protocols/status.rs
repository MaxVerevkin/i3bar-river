use wayland_client::{
    globals::{BindError, GlobalList},
    protocol::wl_output,
    Connection, Dispatch, QueueHandle,
};

use protocol::{zriver_output_status_v1, zriver_status_manager_v1};

#[derive(Debug, Clone)]
pub struct RiverStatusState {
    status_manager: zriver_status_manager_v1::ZriverStatusManagerV1,
}

impl RiverStatusState {
    pub fn new<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Result<Self, BindError>
    where
        D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, (), D> + 'static,
    {
        let status_manager = globals.bind(qh, 1..=4, ())?;
        Ok(Self { status_manager })
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
        RiverOutputStatus(self.status_manager.get_river_output_status(
            output,
            qh,
            RiverStatusData {
                output: output.clone(),
            },
        ))
    }
}

pub trait RiverStatusHandler: Sized {
    fn river_status_state(&mut self) -> &mut RiverStatusState;

    fn focused_tags_updated(&mut self, output: &wl_output::WlOutput, focused: u32);

    fn urgent_tags_updated(&mut self, output: &wl_output::WlOutput, urgent: u32);

    fn views_tags_updated(&mut self, output: &wl_output::WlOutput, tags: Vec<u32>);

    fn layout_name_updated(&mut self, output: &wl_output::WlOutput, layout_name: Option<String>);
}

#[derive(Debug)]
pub struct RiverOutputStatus(zriver_output_status_v1::ZriverOutputStatusV1);

impl Drop for RiverOutputStatus {
    fn drop(&mut self) {
        self.0.destroy();
    }
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
pub struct RiverStatusData {
    output: wl_output::WlOutput,
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
        _: &zriver_output_status_v1::ZriverOutputStatusV1,
        event: zriver_output_status_v1::Event,
        udata: &RiverStatusData,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        use zriver_output_status_v1::Event;

        match event {
            Event::FocusedTags { tags } => data.focused_tags_updated(&udata.output, tags),
            Event::UrgentTags { tags } => data.urgent_tags_updated(&udata.output, tags),
            Event::ViewTags { tags } => {
                let tags = tags
                    .chunks_exact(4)
                    .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                    .collect();
                data.views_tags_updated(&udata.output, tags);
            }
            Event::LayoutName { name } => data.layout_name_updated(&udata.output, Some(name)),
            Event::LayoutNameClear => data.layout_name_updated(&udata.output, None),
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
