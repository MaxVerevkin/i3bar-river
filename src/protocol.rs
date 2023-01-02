#![allow(clippy::enum_variant_names)]

use wayrs_client;
pub use wayrs_client::protocol::*;
wayrs_scanner::generate!("protocols/xdg-shell.xml");
wayrs_scanner::generate!("protocols/wlr-layer-shell-unstable-v1.xml");
wayrs_scanner::generate!("protocols/river-status-unstable-v1.xml");
wayrs_scanner::generate!("protocols/river-control-unstable-v1.xml");
