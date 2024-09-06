use std::collections::HashMap;
use std::io;
use std::os::fd::RawFd;

use anyhow::Result;
use wayrs_client::Connection;

use crate::state::State;

type Callback = Box<dyn FnMut(EventLoopCtx) -> Result<Action>>;

pub struct EventLoopCtx<'a> {
    pub conn: &'a mut Connection<State>,
    pub state: &'a mut State,
}

/// Simple callback-based event loop. Implemented using `poll`.
pub struct EventLoop {
    cbs: HashMap<RawFd, Callback>,
    on_idle: Vec<Callback>,
}

pub enum Action {
    Keep,
    Unregister,
}

impl EventLoop {
    pub fn new() -> Self {
        Self {
            cbs: HashMap::new(),
            on_idle: Vec::new(),
        }
    }

    pub fn register_with_fd<F>(&mut self, fd: RawFd, cb: F)
    where
        F: FnMut(EventLoopCtx) -> Result<Action> + 'static,
    {
        self.cbs.insert(fd, Box::new(cb));
    }

    pub fn add_on_idle<F>(&mut self, cb: F)
    where
        F: FnMut(EventLoopCtx) -> Result<Action> + 'static,
    {
        self.on_idle.push(Box::new(cb));
    }

    pub fn run(&mut self, conn: &mut Connection<State>, state: &mut State) -> Result<()> {
        let mut pollfds = Vec::new();
        let mut on_idle_scratch = Vec::new();

        while !self.cbs.is_empty() {
            pollfds.clear();
            for &fd in self.cbs.keys() {
                pollfds.push(libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                });
            }

            loop {
                let result = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as _, -1) };
                if result == -1 {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(err.into());
                }
                break;
            }

            for fd in &pollfds {
                if fd.revents != 0 {
                    let mut cb = self.cbs.remove(&fd.fd).unwrap();
                    match cb(EventLoopCtx { conn, state })? {
                        Action::Keep => {
                            self.cbs.insert(fd.fd, cb);
                        }
                        Action::Unregister => (),
                    }
                }
            }

            for mut cb in self.on_idle.drain(..) {
                match cb(EventLoopCtx { conn, state })? {
                    Action::Keep => on_idle_scratch.push(cb),
                    Action::Unregister => (),
                }
            }
            self.on_idle.append(&mut on_idle_scratch);
        }
        Ok(())
    }
}
