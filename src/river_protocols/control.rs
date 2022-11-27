use wayland_client::globals::{BindError, GlobalList};
use wayland_client::{protocol::wl_seat, Connection, Dispatch, QueueHandle};

use protocol::{zriver_command_callback_v1, zriver_control_v1};

#[derive(Debug, Clone)]
pub struct RiverControlState {
    control: zriver_control_v1::ZriverControlV1,
}

impl RiverControlState {
    pub fn new<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Result<Self, BindError>
    where
        D: Dispatch<zriver_control_v1::ZriverControlV1, (), D> + 'static,
    {
        let control = globals.bind(qh, 1..=1, ())?;
        Ok(Self { control })
    }

    pub fn run_command<D, Args, Arg>(&self, qh: &QueueHandle<D>, seat: &wl_seat::WlSeat, args: Args)
    where
        D: Dispatch<zriver_command_callback_v1::ZriverCommandCallbackV1, RiverCommandCallbackData>
            + 'static,
        Args: IntoIterator<Item = Arg>,
        Arg: Into<String>,
    {
        for arg in args {
            self.control.add_argument(arg.into());
        }
        self.control
            .run_command(seat, qh, RiverCommandCallbackData {});
    }
}

pub trait RiverControlHandler: Sized {
    fn river_control_state(&mut self) -> &mut RiverControlState;

    fn command_failure(&mut self, conn: &Connection, qh: &QueueHandle<Self>, message: String);

    fn command_success(&mut self, conn: &Connection, qh: &QueueHandle<Self>, message: String);
}

#[derive(Debug)]
pub struct RiverCommandCallbackData {
    // This is empty right now, but may be populated in the future.
}

#[macro_export]
macro_rules! delegate_river_control {
    ($ty: ty) => {
        ::smithay_client_toolkit::reexports::client::delegate_dispatch!($ty: [
            $crate::river_protocols::control::protocol::zriver_control_v1::ZriverControlV1: ()
        ] => $crate::river_protocols::control::RiverControlState);
        ::smithay_client_toolkit::reexports::client::delegate_dispatch!($ty: [
            $crate::river_protocols::control::protocol::zriver_command_callback_v1::ZriverCommandCallbackV1: $crate::river_protocols::control::RiverCommandCallbackData
        ] => $crate::river_protocols::control::RiverControlState);
    };
}

impl<D> Dispatch<zriver_control_v1::ZriverControlV1, (), D> for RiverControlState
where
    D: Dispatch<zriver_control_v1::ZriverControlV1, ()> + RiverControlHandler + 'static,
{
    fn event(
        _: &mut D,
        _: &zriver_control_v1::ZriverControlV1,
        _: zriver_control_v1::Event,
        _: &(),
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

pub mod protocol {
    #![allow(non_upper_case_globals)]

    use smithay_client_toolkit::reexports::client as wayland_client;
    use smithay_client_toolkit::reexports::client::protocol::*;

    pub mod __interfaces {
        use smithay_client_toolkit::reexports::client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/river-control-unstable-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/river-control-unstable-v1.xml");
}
