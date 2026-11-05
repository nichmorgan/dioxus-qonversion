use std::ffi::{c_char, CString};

fn result(work: impl FnOnce() -> Result<(), String>) -> *mut c_char {
    match super::boundary(work) {
        Ok(()) => std::ptr::null_mut(),
        Err(message) => CString::new(message.replace('\0', "?"))
            .expect("NUL bytes replaced")
            .into_raw(),
    }
}

// Only the test library exports these symbols. The production crate is unchanged.
#[no_mangle]
pub extern "C" fn qrc_test_initialize() -> *mut c_char {
    result(super::initialize)
}

#[no_mangle]
pub extern "C" fn qrc_test_query() -> *mut c_char {
    result(super::query)
}

/// # Safety
/// Pointer must be an error allocated above, released exactly once after copy.
#[no_mangle]
pub unsafe extern "C" fn qrc_test_free_error(error: *mut c_char) {
    if !error.is_null() {
        drop(CString::from_raw(error));
    }
}
