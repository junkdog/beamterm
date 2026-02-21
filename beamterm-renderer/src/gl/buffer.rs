use std::slice;

use glow::HasContext;

/// Uploads a single struct to a GL buffer as raw bytes.
///
/// # Safety
/// Requires that T:
/// - Has a stable memory layout (use #[repr(C)] or #[repr(transparent)])
/// - Contains only copy types
/// - Has no padding issues that would cause UB
pub(super) fn buffer_upload_struct<T>(gl: &glow::Context, target: u32, data: &T, usage: u32) {
    unsafe {
        let data_ptr = data as *const T as *const u8;
        let size = size_of::<T>();
        let bytes = slice::from_raw_parts(data_ptr, size);
        gl.buffer_data_u8_slice(target, bytes, usage);
    }
}

/// Uploads an array of elements to a GL buffer as raw bytes.
///
/// # Safety
/// Requires that T:
/// - Has a stable memory layout (use #[repr(C)] or #[repr(transparent)])
/// - Contains only copy types
/// - Has no padding issues that would cause UB
pub(super) fn buffer_upload_array<T>(gl: &glow::Context, target: u32, data: &[T], usage: u32) {
    unsafe {
        let data_ptr = data.as_ptr() as *const u8;
        let size = std::mem::size_of_val(data);
        let bytes = slice::from_raw_parts(data_ptr, size);
        gl.buffer_data_u8_slice(target, bytes, usage);
    }
}
