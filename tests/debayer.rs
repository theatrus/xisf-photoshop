use seiza_fits::{F32ImageData, HeaderValue, WriteHeaderCard};
use seiza_photoshop::{Format, Image, debayer, decode, encode, ffi::*};
use std::ptr;

fn fixture(
    format: Format,
    pattern: Option<&str>,
    x: f64,
    y: f64,
    width: usize,
    height: usize,
    pixels: &[f32],
) -> Vec<u8> {
    let mut headers = vec![
        WriteHeaderCard::new("XBAYROFF", HeaderValue::Float(x)),
        WriteHeaderCard::new("YBAYROFF", HeaderValue::Float(y)),
    ];
    if let Some(p) = pattern {
        headers.push(WriteHeaderCard::new(
            "BAYERPAT",
            HeaderValue::String(p.into()),
        ));
    }
    let mut bytes = Vec::new();
    if format == Format::Fits {
        seiza_fits::write_f32_image_to(
            &mut bytes,
            width,
            height,
            F32ImageData::Mono(pixels),
            &headers,
        )
        .unwrap();
    } else {
        let cfa = pattern
            .map(|p| format!(r#"<ColorFilterArray pattern="{p}" width="2" height="2"/>"#))
            .unwrap_or_default();
        let xml = format!(
            r#"<xisf version="1.0" xmlns="http://www.pixinsight.com/xisf"><Image geometry="{width}:{height}:1" sampleFormat="Float32" colorSpace="Gray" location="attachment:4096:{}">{cfa}<FITSKeyword name="XBAYROFF" value="{x}"/><FITSKeyword name="YBAYROFF" value="{y}"/></Image></xisf>"#,
            pixels.len() * 4
        );
        bytes.extend(b"XISF0100");
        bytes.extend((xml.len() as u32).to_le_bytes());
        bytes.extend([0; 4]);
        bytes.extend(xml.as_bytes());
        bytes.resize(4096, 0);
        bytes.extend(pixels.iter().flat_map(|p| p.to_le_bytes()));
    }
    bytes
}

#[test]
fn all_patterns_offsets_odd_edges_and_hdr_survive_debayer_and_export() {
    for format in [Format::Fits, Format::Xisf] {
        for (id, pattern) in ["RGGB", "BGGR", "GRBG", "GBRG"].iter().enumerate() {
            for x in [-1i32, 0, 1, 2] {
                for y in [-1i32, 0, 1, 2] {
                    let (w, h) = (5, 3);
                    let pixels: Vec<f32> = (0..h)
                        .flat_map(|row| {
                            (0..w).map(move |col| {
                                let index = ((row as i32 + y).rem_euclid(2) * 2
                                    + (col as i32 + x).rem_euclid(2))
                                    as usize;
                                match pattern.as_bytes()[index] {
                                    b'R' => -0.25,
                                    b'G' => 0.5,
                                    _ => 2.0,
                                }
                            })
                        })
                        .collect();
                    let mut image = decode(
                        format,
                        &fixture(format, Some(pattern), x as f64, y as f64, w, h, &pixels),
                    )
                    .unwrap();
                    assert_eq!(image.cfa.pattern, id as u32 + 1);
                    assert_eq!(image.cfa.x_offset, x.rem_euclid(2) as u32);
                    debayer::apply(&mut image, 1).unwrap();
                    assert_eq!(image.planes, 3);
                    let expected: Vec<f32> = [-0.25, 0.5, 2.0]
                        .into_iter()
                        .flat_map(|v| vec![v; w * h])
                        .collect();
                    assert_eq!(image.pixels, expected);
                    let mut encoded = Vec::new();
                    encode(format, &image, &mut encoded).unwrap();
                    let saved = decode(format, &encoded).unwrap();
                    assert_eq!(saved.pixels, expected);
                    assert_eq!(saved.cfa.pattern, 0);
                }
            }
        }
    }
}

#[test]
fn raw_and_auto_preserve_untagged_and_unsupported_mosaics_manual_is_explicit() {
    for format in [Format::Fits, Format::Xisf] {
        for pattern in [None, Some("XXXX"), Some("RGGB")] {
            let bytes = fixture(format, pattern, 0.0, 0.0, 2, 2, &[-0.25, 0.5, 0.5, 2.0]);
            let mut image = decode(format, &bytes).unwrap();
            debayer::apply(&mut image, 0).unwrap();
            assert_eq!(image.planes, 1);
            if pattern != Some("RGGB") {
                debayer::apply(&mut image, 1).unwrap();
                assert_eq!(image.planes, 1);
            }
            debayer::apply(&mut image, 2).unwrap();
            assert_eq!(
                image.pixels,
                [-0.25; 4]
                    .into_iter()
                    .chain([0.5; 4])
                    .chain([2.0; 4])
                    .collect::<Vec<_>>()
            );
            let before = image.pixels.clone();
            debayer::apply(&mut image, 3).unwrap();
            assert_eq!(image.pixels, before);
        }
    }
}

#[test]
fn bilinear_interpolates_missing_channels_and_preserves_measured_samples() {
    let raw = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
    let bytes = fixture(Format::Fits, Some("RGGB"), 0.0, 0.0, 3, 3, &raw);
    let mut image = decode(Format::Fits, &bytes).unwrap();
    debayer::apply(&mut image, 1).unwrap();
    assert_eq!(image.pixels[4], 5.0); // Center red = average of the four red corners.
    assert_eq!(image.pixels[9 + 4], 5.0); // Center green = average of four axial greens.
    assert_eq!(image.pixels[18 + 4], 5.0); // Measured blue retained exactly.
    assert_eq!(image.pixels[0], 1.0);
    assert_eq!(image.pixels[8], 9.0);
}

#[test]
fn malformed_offsets_and_tiny_cfa_fail_without_mutating_pixels() {
    for format in [Format::Fits, Format::Xisf] {
        for (width, height, offset) in [(2, 2, 0.5), (1, 4, 0.0), (4, 1, 0.0)] {
            let mut image = decode(
                format,
                &fixture(format, Some("RGGB"), offset, 0.0, width, height, &[1.0; 4]),
            )
            .unwrap();
            assert!(debayer::apply(&mut image, 1).is_err());
            assert_eq!(image.pixels, [1.0; 4]);
            assert_eq!(image.planes, 1);
            debayer::apply(&mut image, 0).unwrap();
        }
    }
}

#[test]
fn ffi_debayer_updates_view_and_rejects_null_or_invalid_modes() {
    let bytes = fixture(
        Format::Xisf,
        Some("RGGB"),
        0.0,
        0.0,
        2,
        2,
        &[-0.25, 0.5, 0.5, 2.0],
    );
    let mut image: *mut Image = ptr::null_mut();
    let mut error = [0i8; 512];
    unsafe {
        assert_eq!(
            seiza_decode(
                2,
                bytes.as_ptr(),
                bytes.len(),
                &mut image,
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
        let mut view = std::mem::zeroed();
        assert_eq!(seiza_image_view(image, &mut view), 0);
        assert_eq!(view.cfa_pattern, 1);
        assert_ne!(
            seiza_image_debayer(image, 99, error.as_mut_ptr(), error.len()),
            0
        );
        assert_eq!(
            seiza_image_debayer(image, 1, error.as_mut_ptr(), error.len()),
            0
        );
        assert_eq!(seiza_image_view(image, &mut view), 0);
        assert_eq!(view.planes, 3);
        assert_eq!(view.samples, 12);
        assert_eq!(view.cfa_pattern, 0);
        seiza_image_free(image);
        assert_ne!(
            seiza_image_debayer(ptr::null_mut(), 1, error.as_mut_ptr(), error.len()),
            0
        );
    }
}
