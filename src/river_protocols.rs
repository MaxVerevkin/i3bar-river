pub use generated::client1::*;
pub use generated::client2::*;

mod generated {
    // The generated code tends to trigger a lot of warnings
    // so we isolate it into a very permissive module
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(clippy::all)]

    pub mod client1 {
        // These imports are used by the generated code
        use smithay_client_toolkit::reexports::client as wayland_client;
        use wayland_client::protocol::*;
        use wayland_client::sys;
        use wayland_client::{AnonymousObject, Attached, Main, Proxy, ProxyMap};
        use wayland_commons::map::{Object, ObjectMetadata};
        use wayland_commons::smallvec;
        use wayland_commons::wire::{Argument, ArgumentType, Message, MessageDesc};
        use wayland_commons::{Interface, MessageGroup};
        include!(concat!(env!("OUT_DIR"), "/river-status-unstable-v1.rs"));
    }

    pub mod client2 {
        // These imports are used by the generated code
        use smithay_client_toolkit::reexports::client as wayland_client;
        use wayland_client::protocol::*;
        use wayland_client::sys;
        use wayland_client::{AnonymousObject, Attached, Main, Proxy, ProxyMap};
        use wayland_commons::map::{Object, ObjectMetadata};
        use wayland_commons::smallvec;
        use wayland_commons::wire::{Argument, ArgumentType, Message, MessageDesc};
        use wayland_commons::{Interface, MessageGroup};
        include!(concat!(env!("OUT_DIR"), "/river-control-unstable-v1.rs"));
    }
}
