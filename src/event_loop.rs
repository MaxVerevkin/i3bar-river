use std::collections::HashMap;
use std::io;
use std::os::fd::RawFd;

use anyhow::Result;

pub struct EventLoop<D> {
    pollfds: Vec<libc::pollfd>,
    cbs: HashMap<RawFd, Box<dyn FnMut(&mut D) -> Result<Action>>>,
}

pub enum Action {
    Keep,
    Unregister,
}

impl<D> EventLoop<D> {
    pub fn new() -> Self {
        Self {
            pollfds: Vec::new(),
            cbs: HashMap::new(),
        }
    }

    pub fn register_with_fd<F>(&mut self, fd: RawFd, cb: F)
    where
        F: FnMut(&mut D) -> Result<Action> + 'static,
    {
        self.cbs.insert(fd, Box::new(cb));
    }

    pub fn run(&mut self, state: &mut D) -> Result<()> {
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
                match cb(state)? {
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
