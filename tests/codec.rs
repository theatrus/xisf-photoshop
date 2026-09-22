use seiza_photoshop::ffi::*;
use seiza_photoshop::{Format, Image, decode, encode, sample_count};
use std::{
    ffi::{CStr, c_char, c_void},
    ptr,
};

fn fixture(
    bitpix: i32,
    width: usize,
    height: usize,
    planes: usize,
    extra: &[&str],
    pixels: &[u8],
) -> Vec<u8> {
    let mut cards = vec![
        "SIMPLE  =                    T".to_owned(),
        format!("BITPIX  = {bitpix:20}"),
        format!("NAXIS   = {:20}", if planes == 1 { 2 } else { 3 }),
        format!("NAXIS1  = {width:20}"),
        format!("NAXIS2  = {height:20}"),
    ];
    if planes != 1 {
        cards.push(format!("NAXIS3  = {planes:20}"));
    }
    cards.extend(extra.iter().map(|s| s.to_string()));
    cards.push("END".to_owned());
    let mut bytes = Vec::new();
    for card in cards {
        bytes.extend(format!("{card:80}").as_bytes());
    }
    bytes.resize(bytes.len().div_ceil(2880) * 2880, b' ');
    bytes.extend(pixels);
    bytes.resize(bytes.len().div_ceil(2880) * 2880, 0);
    bytes
}

#[test]
fn float_roundtrips_preserve_hdr_negatives_orientation_and_planes() {
    for format in [Format::Fits, Format::Xisf] {
        for planes in [1, 3] {
            let pixels = (0..6 * planes).map(|i| i as f32 / 8.0 - 0.25).collect();
            let image = Image {
                width: 3,
                height: 2,
                planes,
                pixels,
            };
            let mut bytes = Vec::new();
            encode(format, &image, &mut bytes).unwrap();
            let actual = decode(format, &bytes).unwrap();
            assert_eq!((actual.width, actual.height, actual.planes), (3, 2, planes));
            assert_eq!(actual.pixels, image.pixels);
        }
    }
}

#[test]
fn unsigned_integer_samples_use_fixed_full_scale_not_histogram() {
    let bytes = fixture(8, 3, 1, 1, &[], &[0, 128, 255]);
    assert_eq!(
        decode(Format::Fits, &bytes).unwrap().pixels,
        vec![0.0, 128.0 / 255.0, 1.0]
    );
    let pixels: Vec<_> = [-32768i16, 0, 32767]
        .into_iter()
        .flat_map(i16::to_be_bytes)
        .collect();
    let bytes = fixture(16, 3, 1, 1, &["BZERO   =                32768"], &pixels);
    assert_eq!(
        decode(Format::Fits, &bytes).unwrap().pixels,
        vec![0.0, 32768.0 / 65535.0, 1.0]
    );
}

#[test]
fn fits_scaling_is_applied_exactly_once() {
    let pixels: Vec<_> = [1.0f32, 2.0]
        .into_iter()
        .flat_map(f32::to_be_bytes)
        .collect();
    let bytes = fixture(
        -32,
        2,
        1,
        1,
        &[
            "BZERO   =                   10",
            "BSCALE  =                    2",
        ],
        &pixels,
    );
    assert_eq!(
        decode(Format::Fits, &bytes).unwrap().pixels,
        vec![12.0, 14.0]
    );
    let pixels: Vec<_> = [1i16, 2].into_iter().flat_map(i16::to_be_bytes).collect();
    let bytes = fixture(
        16,
        2,
        1,
        1,
        &[
            "BZERO   =                   10",
            "BSCALE  =                    2",
        ],
        &pixels,
    );
    assert_eq!(
        decode(Format::Fits, &bytes).unwrap().pixels,
        vec![12.0, 14.0]
    );
}

#[test]
fn signed_fits16_does_not_lose_negative_samples() {
    let pixels: Vec<_> = [-100i16, 0, 100]
        .into_iter()
        .flat_map(i16::to_be_bytes)
        .collect();
    let bytes = fixture(16, 3, 1, 1, &[], &pixels);
    assert_eq!(
        decode(Format::Fits, &bytes).unwrap().pixels,
        [-100.0, 0.0, 100.0]
    );
}

fn xisf_fixture(format: &str, geometry: &str, attributes: &str, payload: &[u8]) -> Vec<u8> {
    let xml = format!(
        r#"<?xml version="1.0"?><xisf version="1.0" xmlns="http://www.pixinsight.com/xisf"><Image geometry="{geometry}" sampleFormat="{format}" colorSpace="{}" location="attachment:4096:{}" {attributes}/></xisf>"#,
        if geometry.ends_with(":3") {
            "RGB"
        } else {
            "Gray"
        },
        payload.len()
    );
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend((xml.len() as u32).to_le_bytes());
    bytes.extend([0u8; 4]);
    bytes.extend(xml.as_bytes());
    bytes.resize(4096, 0);
    bytes.extend(payload);
    bytes
}

#[test]
fn xisf_unsigned32_and_big_endian_double_conversion() {
    let raw: Vec<_> = [0u32, u32::MAX]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect();
    let bytes = xisf_fixture("UInt32", "2:1:1", r#"byteOrder="big""#, &raw);
    assert_eq!(decode(Format::Xisf, &bytes).unwrap().pixels, [0.0, 1.0]);
    let raw: Vec<_> = [-0.5f64, 1.5]
        .into_iter()
        .flat_map(f64::to_be_bytes)
        .collect();
    let bytes = xisf_fixture("Float64", "2:1:1", r#"byteOrder="big" bounds="0:1""#, &raw);
    assert_eq!(decode(Format::Xisf, &bytes).unwrap().pixels, [-0.5, 1.5]);
}

#[test]
fn compressed_shuffled_xisf_keeps_planar_rgb_samples() {
    use std::io::Write;
    let pixels = [0.0f32, 0.125, 0.25, 0.5, 0.75, 1.0];
    let raw: Vec<_> = pixels.into_iter().flat_map(f32::to_le_bytes).collect();
    let shuffled: Vec<u8> = (0..4)
        .flat_map(|byte| raw.chunks_exact(4).map(move |sample| sample[byte]))
        .collect();
    for codec in ["zlib", "lz4", "zstd"] {
        let compressed = match codec {
            "zlib" => {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(&shuffled).unwrap();
                encoder.finish().unwrap()
            }
            "lz4" => lz4_flex::block::compress(&shuffled),
            _ => zstd::stream::encode_all(shuffled.as_slice(), 1).unwrap(),
        };
        let attrs = format!(
            r#"byteOrder="little" pixelStorage="Planar" compression="{codec}+sh:{}:4""#,
            raw.len()
        );
        let bytes = xisf_fixture("Float32", "2:1:3", &attrs, &compressed);
        assert_eq!(decode(Format::Xisf, &bytes).unwrap().pixels, pixels);
    }
}

#[test]
fn invalid_data_and_unsupported_cubes_are_errors() {
    for format in [Format::Fits, Format::Xisf] {
        assert!(decode(format, b"not an image").is_err());
        let image = Image {
            width: 2,
            height: 1,
            planes: 1,
            pixels: vec![0.0, 1.0],
        };
        let mut bytes = Vec::new();
        encode(format, &image, &mut bytes).unwrap();
        for len in [0, 7, 15, 80] {
            assert!(decode(format, &bytes[..len]).is_err());
        }
    }
    let cube = fixture(8, 1, 1, 4, &[], &[1, 2, 3, 4]);
    assert!(decode(Format::Fits, &cube).unwrap_err().contains("cube"));
    let nan = fixture(-32, 1, 1, 1, &[], &f32::NAN.to_be_bytes());
    assert!(decode(Format::Fits, &nan).unwrap_err().contains("NaN"));
}

#[test]
fn encoding_checks_dimensions_length_and_nonfinite_samples() {
    for format in [Format::Fits, Format::Xisf] {
        for image in [
            Image {
                width: 0,
                height: 1,
                planes: 1,
                pixels: vec![],
            },
            Image {
                width: 1,
                height: 1,
                planes: 4,
                pixels: vec![0.0; 4],
            },
            Image {
                width: 2,
                height: 1,
                planes: 1,
                pixels: vec![0.0],
            },
            Image {
                width: 1,
                height: 1,
                planes: 1,
                pixels: vec![f32::INFINITY],
            },
        ] {
            assert!(encode(format, &image, Vec::new()).is_err());
        }
    }
    assert!(sample_count(usize::MAX, 2, 3).is_err());
}

unsafe extern "C" fn collect(context: *mut c_void, bytes: *const u8, count: usize) -> i32 {
    unsafe {
        (&mut *context.cast::<Vec<u8>>())
            .extend_from_slice(std::slice::from_raw_parts(bytes, count));
    }
    0
}
unsafe extern "C" fn fail(_: *mut c_void, _: *const u8, _: usize) -> i32 {
    1
}

#[test]
fn ffi_ownership_callback_errors_and_null_arguments() {
    unsafe {
        let mut error = [0 as c_char; 128];
        let pixels = [0.1f32, 0.2, 0.3];
        for format in [1, 2] {
            let mut bytes: Vec<u8> = Vec::new();
            assert_eq!(
                seiza_encode(
                    format,
                    1,
                    1,
                    3,
                    pixels.as_ptr(),
                    3,
                    Some(collect),
                    (&mut bytes as *mut Vec<u8>).cast(),
                    error.as_mut_ptr(),
                    error.len()
                ),
                0
            );
            let mut handle = ptr::null_mut();
            assert_eq!(
                seiza_decode(
                    format,
                    bytes.as_ptr(),
                    bytes.len(),
                    &mut handle,
                    error.as_mut_ptr(),
                    error.len()
                ),
                0
            );
            drop(bytes); // decoded pixels must remain valid
            let mut view = std::mem::MaybeUninit::uninit();
            assert_eq!(seiza_image_view(handle, view.as_mut_ptr()), 0);
            let view = view.assume_init();
            assert_eq!(
                (view.width, view.height, view.planes, view.samples),
                (1, 1, 3, 3)
            );
            assert_eq!(
                std::slice::from_raw_parts(view.pixels, view.samples),
                pixels
            );
            seiza_image_free(handle);
            assert_eq!(
                seiza_encode(
                    format,
                    1,
                    1,
                    3,
                    pixels.as_ptr(),
                    3,
                    Some(fail),
                    ptr::null_mut(),
                    error.as_mut_ptr(),
                    error.len()
                ),
                1
            );
            assert!(!CStr::from_ptr(error.as_ptr()).to_bytes().is_empty());
        }
        let mut handle = ptr::null_mut();
        assert_eq!(
            seiza_decode(1, ptr::null(), 0, &mut handle, error.as_mut_ptr(), 1),
            1
        );
        assert_eq!(error[0], 0);
        assert!(handle.is_null());
        assert_eq!(seiza_image_view(ptr::null(), ptr::null_mut()), 1);
        seiza_image_free(ptr::null_mut());
        assert_eq!(
            seiza_decode(
                99,
                pixels.as_ptr().cast(),
                12,
                &mut handle,
                ptr::null_mut(),
                0
            ),
            1
        );
    }
}
