#![allow(dead_code)]

use bitflags::bitflags;
use std::io;
use std::os::raw::{c_int, c_short, c_void};
use std::time::Duration;
use libevent_sys;

mod event;
pub use event::*;

/// Gets used as the boxed context for `EXternCallbackFn`
struct EventCallbackWrapper {
    inner: Box<dyn FnMut(EventFlags)>,
}

extern "C" fn handle_wrapped_callback(_fd: EvutilSocket, event: c_short, ctx: EventCallbackCtx) {
    let cb_ref = unsafe {
        let cb: *mut EventCallbackWrapper = /*std::mem::transmute(*/ ctx as *mut EventCallbackWrapper/*)*/;
        let _cb_ref: &mut EventCallbackWrapper = &mut *cb;
        _cb_ref
    };

    let flags = EventFlags::from_bits_truncate(event as u32);
    (cb_ref.inner)(flags)
}

pub struct Libevent {
    base: EventBase,
}

impl Libevent {
    pub fn new() -> Result<Self, io::Error> {
        EventBase::new()
            .map(|base| Libevent { base })
    }

    // TODO: This should be raw_base, and EventBase should prevent having to use raw altogether.
    /// # Safety
    /// Exposes the event_base handle, which can be used to make any sort of
    /// modifications to the event loop without going through proper checks.
    pub unsafe fn with_base<F: Fn(*mut libevent_sys::event_base) -> c_int>(
        &self,
        f: F
    ) -> c_int
        where
    {
        f(self.base.as_inner_mut())
    }

    /// # Safety
    /// Exposes the event_base handle, which can be used to make any sort of
    /// modifications to the event loop without going through proper checks.
    pub/*(crate)*/ unsafe fn base(&self) -> &EventBase {
        &self.base
    }

    /// # Safety
    /// Exposes the event_base handle, which can be used to make any sort of
    /// modifications to the event loop without going through proper checks.
    pub/*(crate)*/ unsafe fn base_mut(&mut self) -> &mut EventBase {
        &mut self.base
    }

    /// Turns the libevent base once.
    // TODO: any way to show if work was done?
    pub fn turn(&self) -> bool {
        let _retval = self.base.loop_(LoopFlags::NONBLOCK);

        true
    }

    /// Turns the libevent base until exit or timeout duration reached.
    // TODO: any way to show if work was done?
    pub fn run_timeout(&self, timeout: Duration) -> bool {
        let _retval = self.base.loopexit(timeout);
        let _retval = self.base.loop_(LoopFlags::empty());

        true
    }

    /// Turns the libevent base until next active event.
    // TODO: any way to show if work was done?
    pub fn run_until_event(&self) -> bool {
        let _retval = self.base.loop_(LoopFlags::ONCE);

        true
    }

    /// Turns the libevent base until exit.
    // TODO: any way to show if work was done?
    pub fn run(&self) -> bool {
        let _retval = self.base.loop_(LoopFlags::empty());

        true
    }

    pub fn add_interval<F: FnMut(EventFlags) + 'static>(&mut self, interval: Duration, cb: F) -> io::Result<EventHandle> {
        let cb_wrapped = Box::new(EventCallbackWrapper {
            inner: Box::new(cb)
        });

        let ev = unsafe { self.base_mut().event_new(
            None,
            EventFlags::PERSIST,
            handle_wrapped_callback,
            /*unsafe {*/std::mem::transmute(cb_wrapped) /*}*/,
        ) };

        let _ = unsafe {
            self.base().event_add(&ev, interval)
        };

        Ok(ev)
    }
}
