use super::protocol::{zriver_output_status_v1, zriver_status_manager_v1};
use wayland_client::{Connection, Dispatch, QueueHandle};

use smithay_client_toolkit::{
    error::GlobalError,
    globals::{GlobalData, ProvidesBoundGlobal},
    registry::{ProvidesRegistryState, RegistryHandler},
};

use super::{RiverStatusData, RiverStatusHandler, RiverStatusState};

impl<D> RegistryHandler<D> for RiverStatusState
where
    D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, GlobalData>
        + RiverStatusHandler
        + ProvidesRegistryState
        + 'static,
{
    fn ready(data: &mut D, _conn: &Connection, qh: &QueueHandle<D>) {
        data.river_status_state().status_manager =
            data.registry().bind_one(qh, 1..=2, GlobalData).into();
    }
}

impl ProvidesBoundGlobal<zriver_status_manager_v1::ZriverStatusManagerV1, 1> for RiverStatusState {
    fn bound_global(&self) -> Result<zriver_status_manager_v1::ZriverStatusManagerV1, GlobalError> {
        self.status_manager.get().cloned()
    }
}

impl ProvidesBoundGlobal<zriver_status_manager_v1::ZriverStatusManagerV1, 2> for RiverStatusState {
    fn bound_global(&self) -> Result<zriver_status_manager_v1::ZriverStatusManagerV1, GlobalError> {
        self.status_manager.get().cloned()
    }
}

impl<D> Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, GlobalData, D>
    for RiverStatusState
where
    D: Dispatch<zriver_status_manager_v1::ZriverStatusManagerV1, GlobalData>
        + RiverStatusHandler
        + 'static,
{
    fn event(
        _: &mut D,
        _: &zriver_status_manager_v1::ZriverStatusManagerV1,
        _: zriver_status_manager_v1::Event,
        _: &GlobalData,
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
                Event::ViewTags { tags: _ } => (),
            }
        }
    }
}
