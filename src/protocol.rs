#![allow(clippy::enum_variant_names)]

use wayrs_client;
pub use wayrs_client::protocol::*;
pub use wayrs_protocols::wlr_layer_shell_unstable_v1::*;
wayrs_client::scanner::generate!("protocols/river-status-unstable-v1.xml");
wayrs_client::scanner::generate!("protocols/river-control-unstable-v1.xml");
