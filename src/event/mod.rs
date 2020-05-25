mod base;
pub use base::*;

#[allow(clippy::module_inception)]
mod event;
pub use event::*;

use std::ptr::NonNull;

pub trait AsRawEvent {
    fn as_raw(&mut self) -> NonNull<libevent_sys::event>;
}

impl<'a, T> AsRawEvent for &'a T where T: AsRawEvent {}
impl<'a, T> AsRawEvent for &'a mut T where T: AsRawEvent {}
