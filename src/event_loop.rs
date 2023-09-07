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

pub struct EventLoop {
    pollfds: Vec<libc::pollfd>,
    cbs: HashMap<RawFd, Callback>,
}

pub enum Action {
    Keep,
    Unregister,
}

impl EventLoop {
    pub fn new() -> Self {
        Self {
            pollfds: Vec::new(),
            cbs: HashMap::new(),
        }
    }

    pub fn register_with_fd<F>(&mut self, fd: RawFd, cb: F)
    where
        F: FnMut(EventLoopCtx) -> Result<Action> + 'static,
    {
        self.cbs.insert(fd, Box::new(cb));
    }

    pub fn run(&mut self, conn: &mut Connection<State>, state: &mut State) -> Result<()> {
        if self.cbs.is_empty() {
            return Ok(());
        }

        self.pollfds.clear();
        for &fd in self.cbs.keys() {
            self.pollfds.push(libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
        }

        loop {
            let result =
                unsafe { libc::poll(self.pollfds.as_mut_ptr(), self.pollfds.len() as _, -1) };
            if result == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err.into());
            }
            break;
        }

        for fd in &self.pollfds {
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

        Ok(())
    }
}
