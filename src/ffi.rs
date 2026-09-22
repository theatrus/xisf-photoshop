//! C ABI contract is documented in native/seiza_codec.h. All exported fallible
//! operations catch panics; allocations remain owned by the Rust library.

use crate::{Format, Image};
use std::ffi::{c_char, c_void};
use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{ptr, slice};

#[repr(C)]
pub struct ImageView {
    pub width: u32,
    pub height: u32,
    pub planes: u32,
    pub samples: usize,
    pub pixels: *const f32,
}

fn boundary(error: *mut c_char, capacity: usize, f: impl FnOnce() -> crate::Result<()>) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err("Astronomy codec panicked".into()));
    let (status, message) = match result {
        Ok(()) => (0, String::new()),
        Err(e) => (1, e),
    };
    if !error.is_null() && capacity > 0 {
        let len = message.len().min(capacity - 1);
        // SAFETY: caller provides a writable capacity-byte error buffer.
        unsafe {
            ptr::copy_nonoverlapping(message.as_ptr(), error.cast(), len);
            *error.add(len) = 0;
        }
    }
    status
}

/// # Safety
/// `bytes` is readable for `length` bytes; `output` is writable; error is either
/// null or writable for `capacity` bytes. Buffers must not alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_decode(
    format: u32,
    bytes: *const u8,
    length: usize,
    output: *mut *mut Image,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    boundary(error, capacity, || {
        if output.is_null() {
            return Err("Null image output".into());
        }
        unsafe {
            *output = ptr::null_mut();
        }
        if bytes.is_null() || length == 0 || length > isize::MAX as usize {
            return Err("Empty or invalid input buffer".into());
        }
        let image = crate::decode(Format::try_from(format)?, unsafe {
            slice::from_raw_parts(bytes, length)
        })?;
        unsafe {
            *output = Box::into_raw(Box::new(image));
        }
        Ok(())
    })
}

/// # Safety
/// `image` is a live handle from seiza_decode; `view` points to writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_image_view(image: *const Image, view: *mut ImageView) -> i32 {
    if image.is_null() || view.is_null() {
        return 1;
    }
    let image = unsafe { &*image };
    unsafe {
        *view = ImageView {
            width: image.width as u32,
            height: image.height as u32,
            planes: image.planes as u32,
            samples: image.pixels.len(),
            pixels: image.pixels.as_ptr(),
        };
    }
    0
}

/// # Safety
/// `image` is null or a live handle from seiza_decode, freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_image_free(image: *mut Image) {
    if !image.is_null() {
        drop(unsafe { Box::from_raw(image) });
    }
}

pub type WriteCallback = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;
struct CallbackWriter {
    callback: WriteCallback,
    context: *mut c_void,
}
impl Write for CallbackWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if unsafe { (self.callback)(self.context, bytes.as_ptr(), bytes.len()) } == 0 {
            Ok(bytes.len())
        } else {
            Err(io::Error::other(
                "Photoshop file write failed or was cancelled",
            ))
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// # Safety
/// `pixels` contains `samples` f32s. Callback must consume each entire buffer
/// synchronously, return zero on success, and never unwind. Error buffer follows
/// seiza_decode's contract. The caller retains ownership of all inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_encode(
    format: u32,
    width: u32,
    height: u32,
    planes: u32,
    pixels: *const f32,
    samples: usize,
    callback: Option<WriteCallback>,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    boundary(error, capacity, || {
        let count = crate::sample_count(width as usize, height as usize, planes as usize)?;
        if pixels.is_null() || samples != count {
            return Err("Invalid pixel buffer".into());
        }
        let callback = callback.ok_or("Missing write callback")?;
        crate::encode_pixels(
            Format::try_from(format)?,
            width as usize,
            height as usize,
            planes as usize,
            unsafe { slice::from_raw_parts(pixels, samples) },
            CallbackWriter { callback, context },
        )
    })
}
