#![allow(dead_code)]

use bitflags::bitflags;
use std::io;
use std::os::raw::{c_int, c_short, c_void};
use std::ptr::NonNull;
use std::time::Duration;

use super::event::*;

/// A file descriptor in libevent.
pub type EvutilSocket = c_int;

/// The event callback function in libevent.
pub type EventCallbackFn = extern "C" fn(EvutilSocket, EventCallbackFlags, EventCallbackCtx);

/// The event callback's raw context type (void pointer).
pub type EventCallbackCtx = *mut c_void;

/// The event callback's raw flags type.
pub type EventCallbackFlags = c_short;

/// Convenience function for mapping Rust's `Duration` to libevent's `timeval`.
fn to_timeval(duration: Duration) -> libevent_sys::timeval {
    libevent_sys::timeval {
        tv_sec: duration.as_secs() as _,
        tv_usec: duration.subsec_micros() as _,
    }
}

/// Wrapper for libevent's `event_base` which is responsible for executing
/// associated events.
pub struct Base {
    base: NonNull<libevent_sys::event_base>,
}

/// The handle that abstracts over libevent's API in Rust.
impl Base {
    /// Creates a new instance of `Base`.
    pub fn new() -> Result<Self, io::Error> {
        let base = unsafe { libevent_sys::event_base_new() };

        if let Some(base) = NonNull::new(base) {
            Ok(unsafe { Self::from_raw(base) })
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to create libevent base",
            ))
        }
    }

    /// Creates a new instance of `Base` using a raw, non-null `event_base`
    /// pointer.
    ///
    /// # Safety
    ///
    /// This function expects a non-null pointer, and thus does no such checks
    /// internally. Thus the caller is responsible for checking the
    /// `event_base` validity.
    pub unsafe fn from_raw(base: NonNull<libevent_sys::event_base>) -> Self {
        Base { base }
    }

    /// Exposes the raw, non-null `event_base` pointer.
    ///
    /// # Safety
    ///
    /// This function returns a valid, non-null `event_base` pointer which by
    /// itself is safe. However, this function serves as an escape hatch to do
    /// unsafe things.
    pub unsafe fn as_raw(&self) -> NonNull<libevent_sys::event_base> {
        self.base
    }

    /// Wrapper for libevent's `event_base_loop`, which runs the event loop in
    /// a manner defined by the `LoopFlags` input.
    pub fn loop_(&self, flags: LoopFlags) -> ExitReason {
        let exit_code = unsafe {
            libevent_sys::event_base_loop(self.base.as_ptr(), flags.bits() as i32) as i32
        };

        match exit_code {
            0 => {
                unsafe {
                    // Technically mutually-exclusive from `got_break`, but
                    // the check in `event_base_loop` comes first, so the logic
                    // here matches.
                    if libevent_sys::event_base_got_exit(self.base.as_ptr()) != 0i32 {
                        ExitReason::GotExit
                    } else if libevent_sys::event_base_got_break(self.base.as_ptr()) != 0i32 {
                        ExitReason::GotBreak
                    } else {
                        // TODO: This should match flags for `EVLOOP_ONCE`, `_NONBLOCK`, etc.
                        ExitReason::Unknown { flags, exit_code }
                    }
                }
            }
            -1 => ExitReason::Error,
            1 => ExitReason::NoPendingEvents,
            _ => ExitReason::Unknown { flags, exit_code },
        }
    }

    /// Wrapper for libevent's `event_base_loopexit`, which tells the running
    /// event loop to exit after a specified `Duration`.
    pub fn loopexit(&self, timeout: Duration) -> i32 {
        let tv = to_timeval(timeout);
        unsafe {
            let tv_cast = &tv as *const libevent_sys::timeval;
            libevent_sys::event_base_loopexit(self.as_raw().as_ptr(), tv_cast) as i32
        }
    }

    /// Wrapper for libevent's `event_base_loopbreak`, which tells the running
    /// event loop to break immediate.
    pub fn loopbreak(&self) -> i32 {
        unsafe { libevent_sys::event_base_loopbreak(self.as_raw().as_ptr()) as i32 }
    }

    /// Wrapper for libevent's `event_base_loopbcontinue`, which tells the
    /// running event loop to resume searching for active events.
    pub fn loopcontinue(&self) -> i32 {
        unsafe { libevent_sys::event_base_loopcontinue(self.as_raw().as_ptr()) as i32 }
    }

    /// Wrapper for libevent's `event_new`, which allocates and initializes a
    /// new `event` with the given parameters.
    pub fn event_new(
        &mut self,
        fd: Option<EvutilSocket>,
        flags: EventFlags,
        callback: EventCallbackFn,
        callback_ctx: Option<EventCallbackCtx>,
    ) -> EventHandle {
        let fd: EvutilSocket = if let Some(fd) = fd {
            // Actual fd
            fd
        } else {
            // Timer
            -1
        };

        let callback_ctx = if let Some(ctx) = callback_ctx {
            ctx
        } else {
            std::ptr::null::<c_void>() as *mut std::ffi::c_void
        };

        let inner = unsafe {
            libevent_sys::event_new(
                self.as_raw().as_ptr(),
                fd,
                flags.bits() as c_short,
                Some(callback),
                callback_ctx,
            )
        };

        EventHandle::from_raw_unchecked(inner)
    }

    /// Wrapper for libevent's `event_new`, which initializes a pre-allocated
    /// `event` with the given parameters.
    pub fn event_assign(
        &mut self,
        ev: &mut EventHandle,
        fd: Option<EvutilSocket>,
        flags: EventFlags,
        callback: EventCallbackFn,
        callback_ctx: Option<EventCallbackCtx>,
    ) -> c_int {
        let fd: EvutilSocket = if let Some(fd) = fd {
            // Actual fd
            fd
        } else {
            // Timer
            -1
        };

        let callback_ctx = if let Some(ctx) = callback_ctx {
            ctx
        } else {
            std::ptr::null::<c_void>() as *mut std::ffi::c_void
        };

        unsafe {
            libevent_sys::event_assign(
                ev.inner.lock().unwrap().inner.unwrap().as_ptr(),
                self.as_raw().as_ptr(),
                fd,
                flags.bits() as c_short,
                Some(callback),
                callback_ctx,
            )
        }
    }

    /// Wrapper for libevent's `event_add`, which activates an initialized
    /// `event` for a pre-defined `Base` and a given timeout interval.
    pub fn event_add(&self, event: &EventHandle, timeout: Option<Duration>) -> c_int {
        unsafe {
            let p = event.inner.lock().unwrap().inner.unwrap().as_ptr();
            if let Some(tv) = timeout {
                libevent_sys::event_add(p, &to_timeval(tv))
            } else {
                // null timeout means no timeout to libevent
                libevent_sys::event_add(p, std::ptr::null())
            }
        }
    }
}

unsafe impl Send for Base {}

/// Enumerates all possible reasons that the event loop may have stopped
/// running.
pub enum ExitReason {
    GotExit,
    GotBreak,
    Error,
    NoPendingEvents,
    Unknown { flags: LoopFlags, exit_code: i32 },
}

bitflags! {
    /// Flags given to the event loop to alter its behavior.
    pub struct LoopFlags: u32 {
        const ONCE = libevent_sys::EVLOOP_ONCE;
        const NONBLOCK = libevent_sys::EVLOOP_NONBLOCK;
        const NO_EXIT_ON_EMPTY = libevent_sys::EVLOOP_NO_EXIT_ON_EMPTY;
    }
}

bitflags! {
    /// Flags used both as inputs to define activation characteristics of an event,
    /// as well as an output given in the callback as to what triggered event
    /// activation.
    pub struct EventFlags: u32 {
        const TIMEOUT = libevent_sys::EV_TIMEOUT;
        const READ = libevent_sys::EV_READ;
        const WRITE = libevent_sys::EV_WRITE;
        const SIGNAL = libevent_sys::EV_SIGNAL;
        const PERSIST = libevent_sys::EV_PERSIST;
        const ET = libevent_sys::EV_ET;
        const FINALIZE = libevent_sys::EV_FINALIZE;
        const CLOSED = libevent_sys::EV_CLOSED;
    }
}