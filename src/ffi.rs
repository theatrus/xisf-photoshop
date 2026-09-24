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
    pub cfa_pattern: u32,
    pub cfa_x_offset: u32,
    pub cfa_y_offset: u32,
    pub cfa_invalid_offsets: u32,
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
            cfa_pattern: image.cfa.pattern,
            cfa_x_offset: image.cfa.x_offset,
            cfa_y_offset: image.cfa.y_offset,
            cfa_invalid_offsets: u32::from(image.cfa.invalid_offsets),
        };
    }
    0
}

/// # Safety
/// `image` is an exclusively borrowed live decoded image. Error follows seiza_decode.
/// Success may invalidate all previously borrowed pixel views; request a new view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_image_debayer(
    image: *mut Image,
    mode: u32,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    boundary(error, capacity, || {
        let image = unsafe { image.as_mut() }.ok_or("Null image")?;
        crate::debayer::apply(image, mode)
    })
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
    unsafe {
        seiza_encode_depth(
            format, 32, width, height, planes, pixels, samples, callback, context, error, capacity,
        )
    }
}

/// # Safety
/// Same contract as seiza_encode. depth is 32 (Float32) or 16 (UInt16).
/// UInt16 rounds normalized samples to 0..65535 and clips outside 0..1.
/// The caller must obtain the user's consent before lossy conversion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_encode_depth(
    format: u32,
    depth: u32,
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
        let encode = match depth {
            32 => crate::encode_pixels,
            16 => crate::encode_u16_pixels,
            _ => return Err("Output depth must be 16 or 32".into()),
        };
        encode(
            Format::try_from(format)?,
            width as usize,
            height as usize,
            planes as usize,
            unsafe { slice::from_raw_parts(pixels, samples) },
            CallbackWriter { callback, context },
        )
    })
}

/// Serialize the decoded image's metadata as a Photoshop XMP packet.
/// # Safety
/// image is live; callback/context/error follow seiza_encode's contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_image_xmp(
    image: *const Image,
    callback: Option<WriteCallback>,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    boundary(error, capacity, || {
        let image = unsafe { image.as_ref() }.ok_or("Null image")?;
        let callback = callback.ok_or("Missing metadata callback")?;
        CallbackWriter { callback, context }
            .write_all(&image.metadata.xmp()?)
            .map_err(|e| e.to_string())
    })
}

/// Encode pixels with astronomy metadata read from the document's XMP.
/// # Safety
/// Pixel/callback/error contract follows seiza_encode. xmp is readable for
/// xmp_length bytes (or null for zero bytes). The caller owns all buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_encode_with_metadata(
    format: u32,
    depth: u32,
    width: u32,
    height: u32,
    planes: u32,
    pixels: *const f32,
    samples: usize,
    xmp: *const u8,
    xmp_length: usize,
    callback: Option<WriteCallback>,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    unsafe {
        seiza_encode_with_profile(
            format,
            depth,
            width,
            height,
            planes,
            pixels,
            samples,
            xmp,
            xmp_length,
            0,
            ptr::null(),
            0,
            callback,
            context,
            error,
            capacity,
        )
    }
}

/// Read a matching ICC profile from the imported image, after optional debayering.
/// # Safety
/// Image must be live; callback and error follow seiza_image_xmp's contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_image_icc(
    image: *const Image,
    callback: Option<WriteCallback>,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    boundary(error, capacity, || {
        let image = unsafe { image.as_ref() }.ok_or("Missing image")?;
        let profile = image.metadata.icc_profile(image.planes)?;
        CallbackWriter {
            callback: callback.ok_or("Missing write callback")?,
            context,
        }
        .write_all(&profile)
        .map_err(|e| e.to_string())
    })
}

/// Encode with an optional authoritative host ICC profile. If replace_icc is 1,
/// an empty profile removes the source profile; 0 preserves source metadata.
/// # Safety
/// Follows seiza_encode_with_metadata; icc must be readable for icc_length bytes
/// (null is allowed for zero bytes). All buffers remain owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_encode_with_profile(
    format: u32,
    depth: u32,
    width: u32,
    height: u32,
    planes: u32,
    pixels: *const f32,
    samples: usize,
    xmp: *const u8,
    xmp_length: usize,
    replace_icc: u32,
    icc: *const u8,
    icc_length: usize,
    callback: Option<WriteCallback>,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    unsafe {
        seiza_encode_with_options(
            format,
            depth,
            width,
            height,
            planes,
            pixels,
            samples,
            xmp,
            xmp_length,
            replace_icc,
            icc,
            icc_length,
            0,
            callback,
            context,
            error,
            capacity,
        )
    }
}

/// Encode with host ICC settings and optional removal of the astrometric solution.
/// remove_astrometry must be 0 (automatic dimension checks) or 1 (always remove).
/// # Safety
/// Buffer and callback contracts follow seiza_encode_with_profile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seiza_encode_with_options(
    format: u32,
    depth: u32,
    width: u32,
    height: u32,
    planes: u32,
    pixels: *const f32,
    samples: usize,
    xmp: *const u8,
    xmp_length: usize,
    replace_icc: u32,
    icc: *const u8,
    icc_length: usize,
    remove_astrometry: u32,
    callback: Option<WriteCallback>,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    boundary(error, capacity, || {
        let count = crate::sample_count(width as usize, height as usize, planes as usize)?;
        if pixels.is_null()
            || samples != count
            || xmp_length > crate::metadata::LIMIT * 2
            || (xmp.is_null() && xmp_length != 0)
            || replace_icc > 1
            || remove_astrometry > 1
            || icc_length > crate::icc::LIMIT
            || (icc.is_null() && icc_length != 0)
        {
            return Err("Invalid pixel or metadata buffer".into());
        }
        let xmp = if xmp_length == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(xmp, xmp_length) }
        };
        let mut metadata = crate::metadata::Metadata::from_xmp(xmp)?.unwrap_or_default();
        if remove_astrometry == 1 {
            metadata.remove_astrometry()?;
        }
        let profile = if icc_length == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(icc, icc_length) }
        };
        crate::metadata::encode_with_icc(
            Format::try_from(format)?,
            depth,
            width as usize,
            height as usize,
            planes as usize,
            unsafe { slice::from_raw_parts(pixels, samples) },
            &metadata,
            (replace_icc == 1).then_some(profile),
            CallbackWriter {
                callback: callback.ok_or("Missing write callback")?,
                context,
            },
        )
    })
}
