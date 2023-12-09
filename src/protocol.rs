pub use wayrs_client::protocol::*;
pub use wayrs_protocols::fractional_scale_v1::*;
pub use wayrs_protocols::viewporter::*;
pub use wayrs_protocols::wlr_layer_shell_unstable_v1::*;
wayrs_client::generate!("protocols/river-status-unstable-v1.xml");
wayrs_client::generate!("protocols/river-control-unstable-v1.xml");
