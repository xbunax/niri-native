use std::ffi::{CString, c_char, c_int};

use super::screen::SpaceId;
use super::window_server::WindowServerId;

unsafe extern "C" {
    fn rift_native_space_create(display: *const c_char) -> u64;
    fn rift_native_space_focus(display: *const c_char, space: u64) -> c_int;
    fn rift_native_space_move(ids: *const u32, count: usize, space: u64) -> c_int;
}

pub(crate) fn create(display: &str) -> Option<SpaceId> {
    let display = CString::new(display).ok()?;
    let id = unsafe { rift_native_space_create(display.as_ptr()) };
    (id != 0).then(|| SpaceId::new(id))
}

pub(crate) fn focus(display: &str, space: SpaceId) -> bool {
    let Ok(display) = CString::new(display) else {
        return false;
    };
    unsafe { rift_native_space_focus(display.as_ptr(), space.get()) != 0 }
}

pub(crate) fn move_windows(ids: &[WindowServerId], space: SpaceId) -> bool {
    let ids: Vec<u32> = ids.iter().map(|id| id.0).collect();
    !ids.is_empty() && unsafe { rift_native_space_move(ids.as_ptr(), ids.len(), space.get()) != 0 }
}
