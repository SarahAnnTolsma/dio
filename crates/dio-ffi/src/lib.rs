//! C-compatible FFI bindings for dio.
//!
//! Exposes `dio_deobfuscate` and `dio_free_string` for use from C, C++,
//! .NET (P/Invoke), and Java (JNI/JNA).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Deobfuscate JavaScript source code.
///
/// # Safety
///
/// - `source` must be a valid pointer to a null-terminated UTF-8 string.
/// - The returned string must be freed with `dio_free_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dio_deobfuscate(source: *const c_char) -> *mut c_char {
    if source.is_null() {
        return std::ptr::null_mut();
    }

    let c_str = unsafe { CStr::from_ptr(source) };
    let source_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let result = dio_core::deobfuscate(source_str);

    match CString::new(result) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string previously returned by `dio_deobfuscate`.
///
/// # Safety
///
/// - `string` must be a pointer returned by `dio_deobfuscate`, or null.
/// - Must not be called more than once for the same pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dio_free_string(string: *mut c_char) {
    if !string.is_null() {
        drop(unsafe { CString::from_raw(string) });
    }
}
