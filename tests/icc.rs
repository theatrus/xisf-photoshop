use base64::{Engine, engine::general_purpose::STANDARD as B64};
use seiza_photoshop::{
    Format, decode, icc,
    metadata::{self, Metadata},
};
use sha2::Digest;
use std::io::Write;

// A structural test envelope, not a colorimetric profile. Real installed profiles
// are exercised separately in Photoshop without redistributing third-party data.
fn profile(planes: usize, marker: u8) -> Vec<u8> {
    let mut data = vec![0; 160];
    data[..4].copy_from_slice(&160u32.to_be_bytes());
    data[16..20].copy_from_slice(if planes == 1 { b"GRAY" } else { b"RGB " });
    data[36..40].copy_from_slice(b"acsp");
    data[80] = marker;
    data[128..132].copy_from_slice(&1u32.to_be_bytes());
    data[132..136].copy_from_slice(b"desc");
    data[136..140].copy_from_slice(&144u32.to_be_bytes());
    data[140..144].copy_from_slice(&16u32.to_be_bytes());
    data
}
fn fixture(element: &str, block: &[u8], planes: usize) -> Vec<u8> {
    let color = if planes == 1 { "Gray" } else { "RGB" };
    let xml = format!(
        r#"<xisf version="1.0"><Image geometry="2:2:{planes}" sampleFormat="Float32" colorSpace="{color}" location="attachment:4096:{}">{element}<Property id="Source" type="String">keep me</Property></Image><Image geometry="1:1:1" sampleFormat="Float32" location="attachment:4096:4"><ICCProfile location="inline:base64">broken ignored profile</ICCProfile></Image></xisf>"#,
        planes * 16
    );
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend((xml.len() as u32).to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(xml.as_bytes());
    bytes.resize(4096, 0);
    for _ in 0..planes {
        for x in [0f32, 0.25, 0.5, 1.0] {
            bytes.extend(x.to_le_bytes());
        }
    }
    bytes.resize(8192, 0);
    bytes.extend(block);
    bytes
}
fn extract(element: &str, block: &[u8], planes: usize) -> Result<Vec<u8>, String> {
    decode(Format::Xisf, &fixture(element, block, planes))?
        .metadata
        .icc_profile(planes)
}
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}
fn xml(bytes: &[u8]) -> &str {
    let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    std::str::from_utf8(&bytes[16..16 + size]).unwrap()
}

#[test]
fn attached_inline_and_embedded_profiles_are_exact_for_gray_and_rgb() {
    for planes in [1, 3] {
        let data = profile(planes, 1);
        let hex: String = data.iter().map(|v| format!("{v:02x}")).collect();
        for element in [
            format!(r#"<ICCProfile location="attachment:8192:{}"/>"#, data.len()),
            format!(
                r#"<ICCProfile location="inline:base64"> {} </ICCProfile>"#,
                B64.encode(&data)
            ),
            format!(r#"<ICCProfile location="inline:hex">{hex}</ICCProfile>"#),
            format!(
                r#"<ICCProfile location="embedded"><Data encoding="base64">{}</Data></ICCProfile>"#,
                B64.encode(&data)
            ),
        ] {
            assert_eq!(extract(&element, &data, planes).unwrap(), data);
        }
    }
}

#[test]
fn compressed_profiles_verify_checksums_and_unshuffle() {
    let data = profile(3, 7);
    for codec in ["zlib", "lz4", "lz4hc", "zstd"] {
        for shuffle in [false, true] {
            let input = if shuffle {
                (0..4)
                    .flat_map(|byte| data.as_chunks::<4>().0.iter().map(move |item| item[byte]))
                    .collect::<Vec<_>>()
            } else {
                data.clone()
            };
            let packed = match codec {
                "zlib" => zlib(&input),
                "zstd" => zstd::stream::encode_all(input.as_slice(), 1).unwrap(),
                _ => lz4_flex::block::compress(&input),
            };
            let compression = if shuffle {
                format!("{codec}+sh:{}:4", data.len())
            } else {
                format!("{codec}:{}", data.len())
            };
            let checksum = sha2::Sha256::digest(&packed)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let element = format!(
                r#"<ICCProfile location="attachment:8192:{}" compression="{compression}" checksum="sha-256:{checksum}"/>"#,
                packed.len()
            );
            assert_eq!(extract(&element, &packed, 3).unwrap(), data);
            let mut corrupted = packed.clone();
            corrupted[0] ^= 1;
            assert!(
                extract(&element, &corrupted, 3)
                    .unwrap_err()
                    .contains("checksum")
            );
            let element = format!(
                r#"<ICCProfile location="embedded"><Data encoding="base64" compression="{compression}" checksum="sha-256:{checksum}">{}</Data></ICCProfile>"#,
                B64.encode(&packed)
            );
            assert_eq!(extract(&element, &[], 3).unwrap(), data);
        }
    }
    for (name, digest) in [
        (
            "sha-1",
            sha1::Sha1::digest(&data)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        ),
        (
            "sha-512",
            sha2::Sha512::digest(&data)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        ),
    ] {
        assert_eq!(
            extract(
                &format!(
                    r#"<ICCProfile location="attachment:8192:160" checksum="{name}:{digest}"/>"#
                ),
                &data,
                3
            )
            .unwrap(),
            data
        );
    }
}

#[test]
fn compressed_subblocks_include_uncompressed_fallback_blocks() {
    let data = profile(3, 1);
    let first = zlib(&data[..128]);
    let mut packed = first.clone();
    packed.extend(&data[128..]);
    let element = format!(
        r#"<ICCProfile location="attachment:8192:{}" compression="zlib:160" subblocks="{},128:32,32"/>"#,
        packed.len(),
        first.len()
    );
    assert_eq!(extract(&element, &packed, 3).unwrap(), data);
}

#[test]
fn host_profile_replaces_source_or_removes_it_without_changing_pixels_or_other_metadata() {
    let original = profile(3, 1);
    let replacement = profile(3, 2);
    let image = decode(
        Format::Xisf,
        &fixture(
            r#"<ICCProfile location="attachment:8192:160"/>"#,
            &original,
            3,
        ),
    )
    .unwrap();
    let metadata = Metadata::from_xmp(&image.metadata.xmp().unwrap())
        .unwrap()
        .unwrap();
    for depth in [16, 32] {
        for host_profile in [None, Some(replacement.as_slice()), Some([].as_slice())] {
            let mut out = Vec::new();
            metadata::encode_with_icc(
                Format::Xisf,
                depth,
                2,
                2,
                3,
                &image.pixels,
                &metadata,
                host_profile,
                &mut out,
            )
            .unwrap();
            let reopened = decode(Format::Xisf, &out).unwrap();
            assert_eq!(
                reopened.metadata.icc_profile(3).unwrap(),
                host_profile.unwrap_or(&original)
            );
            assert!(xml(&out).contains("keep me"));
            assert_eq!(
                xml(&out).matches("<ICCProfile").count(),
                if host_profile == Some(&[]) { 0 } else { 1 }
            );
            if host_profile.is_some() {
                assert!(!out.windows(original.len()).any(|v| v == original));
            }
            for (got, wanted) in reopened.pixels.iter().zip(&image.pixels) {
                assert!((got - wanted).abs() < 0.00002);
            }
        }
    }
    assert_eq!(image.metadata.icc_profile(3).unwrap(), original);
    let mut out = Vec::new();
    metadata::encode_with_icc(
        Format::Xisf,
        32,
        2,
        2,
        3,
        &image.pixels,
        &Metadata::default(),
        Some(&replacement),
        &mut out,
    )
    .unwrap();
    assert_eq!(
        decode(Format::Xisf, &out)
            .unwrap()
            .metadata
            .icc_profile(3)
            .unwrap(),
        replacement
    );
}

#[test]
fn color_model_mismatch_does_not_assign_an_incorrect_profile() {
    let data = profile(1, 1);
    let mut image = decode(
        Format::Xisf,
        &fixture(r#"<ICCProfile location="attachment:8192:160"/>"#, &data, 1),
    )
    .unwrap();
    assert_eq!(image.metadata.icc_profile(1).unwrap(), data);
    seiza_photoshop::debayer::apply(&mut image, 2).unwrap();
    assert!(image.metadata.icc_profile(image.planes).unwrap().is_empty());
    let mut out = Vec::new();
    assert!(
        metadata::encode_with_icc(
            Format::Xisf,
            32,
            2,
            2,
            3,
            &image.pixels,
            &image.metadata,
            Some(&data),
            &mut out
        )
        .is_err()
    );
    assert!(out.is_empty());
    assert!(extract("", &[], 1).unwrap().is_empty());
}

#[test]
fn malformed_and_oversized_profiles_fail_explicitly() {
    let good = profile(3, 1);
    for mutate in [0, 36, 128, 136, 140] {
        let mut bad = good.clone();
        bad[mutate] = 255;
        assert!(icc::validate(&bad).is_err());
    }
    let element = r#"<ICCProfile location="attachment:8192:160"/>"#;
    assert!(extract(&format!("{element}{element}"), &good, 3).is_err());
    assert!(extract(element, &good[..159], 3).is_err());
    for compression in [
        "zlib:999999999",
        "zlib:0",
        "zlib+sh:160:0",
        "zlib+sh:160:3",
        "lz4:159",
        "unknown:160",
    ] {
        let packed = zlib(&good);
        let element = format!(
            r#"<ICCProfile location="attachment:8192:{}" compression="{compression}"/>"#,
            packed.len()
        );
        assert!(extract(&element, &packed, 3).is_err(), "{compression}");
    }
}

#[test]
fn icc_ffi_rejects_invalid_buffers_and_propagates_callback_errors() {
    use seiza_photoshop::ffi::{seiza_encode_with_profile, seiza_image_icc};
    use std::{ffi::c_void, ptr};
    unsafe extern "C" fn fail(_: *mut c_void, _: *const u8, _: usize) -> i32 {
        1
    }
    let bytes = profile(3, 1);
    let image = decode(
        Format::Xisf,
        &fixture(r#"<ICCProfile location="attachment:8192:160"/>"#, &bytes, 3),
    )
    .unwrap();
    let mut error = [0i8; 256];
    unsafe {
        assert_ne!(
            seiza_image_icc(
                ptr::null(),
                Some(fail),
                ptr::null_mut(),
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
        assert_ne!(
            seiza_image_icc(
                &image,
                None,
                ptr::null_mut(),
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
        assert_ne!(
            seiza_image_icc(
                &image,
                Some(fail),
                ptr::null_mut(),
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
        for (flag, profile_ptr, length) in [
            (1, ptr::null(), 1),
            (1, bytes.as_ptr(), icc::LIMIT + 1),
            (2, bytes.as_ptr(), bytes.len()),
        ] {
            assert_ne!(
                seiza_encode_with_profile(
                    2,
                    32,
                    2,
                    2,
                    3,
                    image.pixels.as_ptr(),
                    image.pixels.len(),
                    ptr::null(),
                    0,
                    flag,
                    profile_ptr,
                    length,
                    Some(fail),
                    ptr::null_mut(),
                    error.as_mut_ptr(),
                    error.len()
                ),
                0
            );
            assert!(
                std::ffi::CStr::from_ptr(error.as_ptr())
                    .to_str()
                    .unwrap()
                    .contains("buffer")
            );
        }
    }
}
