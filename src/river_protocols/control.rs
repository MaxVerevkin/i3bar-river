pub mod protocol {
    use smithay_client_toolkit::reexports::client as wayland_client;
    use smithay_client_toolkit::reexports::client::protocol::*;

    pub mod __interfaces {
        use smithay_client_toolkit::reexports::client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/river-control-unstable-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/river-control-unstable-v1.xml");
}

mod dispatch;

use std::sync::Arc;

use smithay_client_toolkit::globals::GlobalData;
use wayland_client::{protocol::wl_seat, Connection, Dispatch, QueueHandle};

use protocol::{zriver_command_callback_v1, zriver_control_v1};

use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::registry::GlobalProxy;

#[derive(Debug, Clone)]
pub struct RiverControlState {
    control: Arc<GlobalProxy<zriver_control_v1::ZriverControlV1>>,
}

impl RiverControlState {
    pub fn new() -> Self {
        Self {
            control: Arc::new(GlobalProxy::NotReady),
        }
    }

    pub fn is_available(&self) -> bool {
        self.control.get().is_ok()
    }

    pub fn control(&self) -> Result<&zriver_control_v1::ZriverControlV1, GlobalError> {
        self.control.get()
    }

    pub fn run_command<D, Args, Arg>(
        &self,
        qh: &QueueHandle<D>,
        seat: &wl_seat::WlSeat,
        args: Args,
    ) -> Result<(), GlobalError>
    where
        D: Dispatch<zriver_control_v1::ZriverControlV1, GlobalData>
            + Dispatch<zriver_command_callback_v1::ZriverCommandCallbackV1, RiverCommandCallbackData>
            + 'static,
        Args: IntoIterator<Item = Arg>,
        Arg: Into<String>,
    {
        let control = self.control()?;
        for arg in args {
            control.add_argument(arg.into());
        }
        control.run_command(seat, qh, RiverCommandCallbackData {})?;
        Ok(())
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
            $crate::river_protocols::control::protocol::zriver_control_v1::ZriverControlV1: ::smithay_client_toolkit::globals::GlobalData,
            $crate::river_protocols::control::protocol::zriver_command_callback_v1::ZriverCommandCallbackV1: $crate::river_protocols::control::RiverCommandCallbackData,
        ] => $crate::river_protocols::control::RiverControlState);
    };
}
