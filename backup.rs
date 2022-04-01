use wayland_client::{
    protocol::{wl_buffer, wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool},
    Connection, ConnectionHandle, Dispatch, QueueHandle, WEnum,
};

// use wayland_protocols::unstable::xdg_output::v1::client::zxdg_output_manager_v1 as xdg_output_manager;
// use wayland_protocols::wlr::unstable::layer_shell::v1::client::zwlr_layer_shell_v1 as wlr_layer_shell;
use wayland_protocols::xdg_shell::client::xdg_wm_base;

use anyhow::Result;

use std::{
    collections::HashMap,
    os::unix::prelude::{AsRawFd, RawFd},
};
// use std::os::unix::io::AsRawFd;
// use std::sync::atomic::{AtomicU32, Ordering};
// use std::sync::Arc;

fn main() -> Result<()> {
    let conn = Connection::connect_to_env()?;
    let display = conn.handle().display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut state = BarState::default();
    display.get_registry(&mut conn.handle(), &qh, ())?;
    conn.roundtrip()?;
    event_queue.dispatch_pending(&mut state)?;
    state.assert_init();

    let mut b = Buffer::create(
        &mut conn.handle(),
        &qh,
        state.wl_shm.as_ref().unwrap(),
        400,
        400,
    )?;

    for p in &*b.data {
        *p = 100;
    }

    loop {
        event_queue.blocking_dispatch(&mut state)?;
    }
}

#[derive(Debug, Default)]
struct BarState {
    wl_shm: Option<wl_shm::WlShm>,
    wl_compositor: Option<wl_compositor::WlCompositor>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
}

impl BarState {
    fn assert_init(&self) {
        assert!(
            self.wl_shm.is_some() && self.wl_compositor.is_some() && self.xdg_wm_base.is_some()
        );
    }
}

impl Dispatch<wl_registry::WlRegistry> for BarState {
    type UserData = ();

    fn event(
        &mut self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &Self::UserData,
        conn: &mut ConnectionHandle,
        qh: &QueueHandle<BarState>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_shm" => {
                    self.wl_shm = Some(registry.bind(conn, name, version, qh, ()).unwrap());
                }
                "wl_compositor" => {
                    self.wl_compositor = Some(registry.bind(conn, name, version, qh, ()).unwrap());
                }
                "xdg_wm_base" => {
                    self.xdg_wm_base = Some(registry.bind(conn, name, version, qh, ()).unwrap());
                }
                _ => (),
            },
            wl_registry::Event::GlobalRemove { name } => {
                eprintln!("Removed: {name}");
            }
            _ => (),
        }
    }
}

impl Dispatch<wl_shm::WlShm> for BarState {
    type UserData = ();
    fn event(
        &mut self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &Self::UserData,
        _: &mut ConnectionHandle,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_compositor::WlCompositor> for BarState {
    type UserData = ();
    fn event(
        &mut self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &Self::UserData,
        _: &mut ConnectionHandle,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase> for BarState {
    type UserData = ();
    fn event(
        &mut self,
        xdg_wm_base: &xdg_wm_base::XdgWmBase,
        e: xdg_wm_base::Event,
        _: &Self::UserData,
        conn: &mut ConnectionHandle,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = e {
            xdg_wm_base.pong(conn, serial);
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool> for BarState {
    type UserData = ();
    fn event(
        &mut self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &Self::UserData,
        _: &mut ConnectionHandle,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer> for BarState {
    type UserData = ();
    fn event(
        &mut self,
        _: &wl_buffer::WlBuffer,
        e: wl_buffer::Event,
        _: &Self::UserData,
        _: &mut ConnectionHandle,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = e {
            todo!()
        }
    }
}

#[derive(Debug)]
struct Buffer {
    width: u32,
    height: u32,
    data: memmap::MmapMut,
    wl_buffer: wl_buffer::WlBuffer,
}

impl Buffer {
    fn create(
        conn: &mut ConnectionHandle,
        qh: &QueueHandle<BarState>,
        shm: &wl_shm::WlShm,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let stride = width * 4;
        let size = height * stride;
        let (data, fd) = alloc_shm("buffer", size as u64)?;
        let pool = shm.create_pool(conn, fd.as_raw_fd(), size as i32, qh, ())?;
        let wl_buffer = pool.create_buffer(
            conn,
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            qh,
            (),
        )?;
        pool.destroy(conn);
        Ok(Self {
            width,
            height,
            data,
            wl_buffer,
        })
    }
}

fn alloc_shm(name: &str, size: u64) -> Result<(memmap::MmapMut, impl AsRawFd)> {
    let file = memfd::MemfdOptions::new().create(name)?;
    file.as_file().set_len(size)?;
    let mmap = unsafe { memmap::MmapMut::map_mut(file.as_file())? };
    Ok((mmap, file))
}
