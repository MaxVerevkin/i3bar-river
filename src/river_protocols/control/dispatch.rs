use std::sync::Arc;

use super::protocol::{zriver_command_callback_v1, zriver_control_v1};
use wayland_client::{Connection, Dispatch, QueueHandle};

use smithay_client_toolkit::{
    error::GlobalError,
    globals::{GlobalData, ProvidesBoundGlobal},
    registry::{ProvidesRegistryState, RegistryHandler},
};

use super::{RiverCommandCallbackData, RiverControlHandler, RiverControlState};

impl<D> RegistryHandler<D> for RiverControlState
where
    D: Dispatch<zriver_control_v1::ZriverControlV1, GlobalData>
        + RiverControlHandler
        + ProvidesRegistryState
        + 'static,
{
    fn ready(data: &mut D, _conn: &Connection, qh: &QueueHandle<D>) {
        data.river_control_state().control =
            Arc::new(data.registry().bind_one(qh, 1..=1, GlobalData).into());
    }
}

impl ProvidesBoundGlobal<zriver_control_v1::ZriverControlV1, 1> for RiverControlState {
    fn bound_global(&self) -> Result<zriver_control_v1::ZriverControlV1, GlobalError> {
        self.control.get().cloned()
    }
}

impl<D> Dispatch<zriver_control_v1::ZriverControlV1, GlobalData, D> for RiverControlState
where
    D: Dispatch<zriver_control_v1::ZriverControlV1, GlobalData> + RiverControlHandler + 'static,
{
    fn event(
        _: &mut D,
        _: &zriver_control_v1::ZriverControlV1,
        _: zriver_control_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("zriver_control_v1 has no events")
    }
}

impl<D> Dispatch<zriver_command_callback_v1::ZriverCommandCallbackV1, RiverCommandCallbackData, D>
    for RiverControlState
where
    D: Dispatch<zriver_command_callback_v1::ZriverCommandCallbackV1, RiverCommandCallbackData>
        + RiverControlHandler
        + 'static,
{
    fn event(
        data: &mut D,
        _callback: &zriver_command_callback_v1::ZriverCommandCallbackV1,
        event: zriver_command_callback_v1::Event,
        _udata: &RiverCommandCallbackData,
        conn: &Connection,
        qh: &QueueHandle<D>,
    ) {
        use zriver_command_callback_v1::Event;
        match event {
            Event::Success { output } => data.command_success(conn, qh, output),
            Event::Failure { failure_message } => data.command_success(conn, qh, failure_message),
        }
    }
}
