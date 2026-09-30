use seiza_photoshop::{
    Format, decode, ffi,
    metadata::{self, Metadata, XisfCompression},
};

fn xml(bytes: &[u8]) -> &str {
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    std::str::from_utf8(&bytes[16..16 + length]).unwrap()
}

fn block<'a>(bytes: &'a [u8], node: roxmltree::Node<'_, '_>) -> &'a [u8] {
    let location: Vec<_> = node.attribute("location").unwrap().split(':').collect();
    assert_eq!(location[0], "attachment");
    let start: usize = location[1].parse().unwrap();
    let length: usize = location[2].parse().unwrap();
    &bytes[start..start + length]
}

#[test]
fn zstd_is_bit_exact_for_all_uint16_codes_and_float32_hdr_gray_and_rgb() {
    for planes in [1, 3] {
        for depth in [16, 32] {
            let pixels: Vec<_> = (0..65536 * planes)
                .map(|i| {
                    if depth == 16 {
                        (i % 65536) as f32 / 65535.0
                    } else {
                        // Include signed zero, subnormal, negative and HDR samples.
                        [
                            -0.0,
                            f32::from_bits(1),
                            -12.75,
                            0.12345679,
                            400.25,
                            f32::MAX,
                        ][i % 6]
                    }
                })
                .collect();
            let mut plain = Vec::new();
            let mut compressed = Vec::new();
            for (compression, output) in [
                (XisfCompression::None, &mut plain),
                (XisfCompression::Zstandard, &mut compressed),
            ] {
                metadata::encode_with_compression(
                    Format::Xisf,
                    depth,
                    256,
                    256,
                    planes,
                    &pixels,
                    &Metadata::default(),
                    None,
                    compression,
                    output,
                )
                .unwrap();
            }
            let plain_doc = roxmltree::Document::parse(xml(&plain)).unwrap();
            let doc = roxmltree::Document::parse(xml(&compressed)).unwrap();
            let image = doc.descendants().find(|n| n.has_tag_name("Image")).unwrap();
            assert_eq!(
                image.attribute("compression"),
                Some(format!("zstd:{}", pixels.len() * (depth as usize / 8)).as_str())
            );
            let raw = zstd::stream::decode_all(block(&compressed, image)).unwrap();
            assert_eq!(
                raw,
                block(
                    &plain,
                    plain_doc
                        .descendants()
                        .find(|n| n.has_tag_name("Image"))
                        .unwrap()
                )
            );
            if depth == 16 {
                for (i, bytes) in raw.as_chunks::<2>().0.iter().enumerate() {
                    assert_eq!(u16::from_le_bytes(*bytes), (i % 65536) as u16);
                }
            } else {
                for (value, bytes) in pixels.iter().zip(raw.as_chunks::<4>().0) {
                    assert_eq!(value.to_bits(), u32::from_le_bytes(*bytes));
                }
            }
            let reopened = decode(Format::Xisf, &compressed).unwrap();
            assert_eq!(
                (reopened.width, reopened.height, reopened.planes),
                (256, 256, planes)
            );
            assert_eq!(reopened.pixels, pixels);
            // Exhaustive UInt16 ramps need not compress; repeated floats should.
            if depth == 32 {
                assert!(compressed.len() < plain.len());
            }
            // Saving an imported compressed file with None must remove old codec attributes.
            let mut uncompressed = Vec::new();
            metadata::encode(
                Format::Xisf,
                depth,
                256,
                256,
                planes,
                &reopened.pixels,
                &reopened.metadata,
                &mut uncompressed,
            )
            .unwrap();
            let doc = roxmltree::Document::parse(xml(&uncompressed)).unwrap();
            let image = doc.descendants().find(|n| n.has_tag_name("Image")).unwrap();
            assert!(image.attribute("compression").is_none());
            // The upstream reader canonicalizes negative zero during physical
            // value conversion. Compression itself was checked bit-for-bit above.
            assert_eq!(
                decode(Format::Xisf, &uncompressed).unwrap().pixels,
                reopened.pixels
            );
        }
    }
}

#[test]
fn compressed_pixels_relocate_metadata_and_host_profile_without_changing_them() {
    let attached = zstd::stream::encode_all(&b"retained binary property"[..], 1).unwrap();
    let source_xml = format!(
        r#"<xisf version="1.0"><Image geometry="1:1:1" sampleFormat="Float32" colorSpace="Gray" location="attachment:4096:4"><Property id="Test:Data" type="UI8Vector" length="24" compression="zstd:24" location="attachment:4100:{}"/><FITSKeyword name="EXPTIME" value="180"/><FITSKeyword name="CRPIX1" value="1"/></Image></xisf>"#,
        attached.len()
    );
    let mut original = b"XISF0100".to_vec();
    original.extend((source_xml.len() as u32).to_le_bytes());
    original.extend([0; 4]);
    original.extend(source_xml.as_bytes());
    original.resize(4096, 0);
    original.extend(0.75f32.to_le_bytes());
    original.extend(&attached);
    let source = decode(Format::Xisf, &original).unwrap();
    let metadata = Metadata::from_xmp(&source.metadata.xmp().unwrap())
        .unwrap()
        .unwrap();
    let mut profile = vec![0; 160];
    profile[..4].copy_from_slice(&160u32.to_be_bytes());
    profile[16..20].copy_from_slice(b"GRAY");
    profile[36..40].copy_from_slice(b"acsp");
    profile[128..132].copy_from_slice(&1u32.to_be_bytes());
    profile[132..136].copy_from_slice(b"desc");
    profile[136..140].copy_from_slice(&144u32.to_be_bytes());
    profile[140..144].copy_from_slice(&16u32.to_be_bytes());
    for depth in [16, 32] {
        let mut output = Vec::new();
        metadata::encode_with_compression(
            Format::Xisf,
            depth,
            2,
            1,
            1,
            &[0.25, 0.75],
            &metadata,
            Some(&profile),
            XisfCompression::Zstandard,
            &mut output,
        )
        .unwrap();
        let doc = roxmltree::Document::parse(xml(&output)).unwrap();
        let property = doc
            .descendants()
            .find(|n| n.attribute("id") == Some("Test:Data"))
            .unwrap();
        assert_eq!(block(&output, property), attached);
        assert_eq!(property.attribute("compression"), Some("zstd:24"));
        assert!(
            doc.descendants()
                .any(|n| n.attribute("name") == Some("EXPTIME"))
        );
        assert!(
            !doc.descendants()
                .any(|n| n.attribute("name") == Some("CRPIX1"))
        );
        assert_eq!(
            block(
                &output,
                doc.descendants()
                    .find(|n| n.has_tag_name("ICCProfile"))
                    .unwrap()
            ),
            profile
        );
        let reopened = decode(Format::Xisf, &output).unwrap();
        assert_eq!(reopened.metadata.icc_profile(1).unwrap(), profile);
        for (value, expected) in reopened.pixels.iter().zip([0.25, 0.75]) {
            assert!((value - expected).abs() < 1.0 / 65535.0);
        }
    }
}

#[test]
fn compression_ffi_rejects_invalid_options_and_reports_write_failure() {
    unsafe extern "C" fn fail(_: *mut std::ffi::c_void, _: *const u8, _: usize) -> i32 {
        -1
    }
    unsafe extern "C" fn collect(ctx: *mut std::ffi::c_void, bytes: *const u8, len: usize) -> i32 {
        unsafe {
            (&mut *ctx.cast::<Vec<u8>>()).extend_from_slice(std::slice::from_raw_parts(bytes, len));
        }
        0
    }
    for (format, compression, fail_write) in [
        (2, 0, false),
        (2, 1, false),
        (2, 2, false),
        (1, 1, false),
        (2, 1, true),
    ] {
        let mut output = Vec::<u8>::new();
        let mut error = [0i8; 512];
        let result = unsafe {
            ffi::seiza_encode_with_compression(
                format,
                32,
                1,
                1,
                1,
                [0.5].as_ptr(),
                1,
                std::ptr::null(),
                0,
                0,
                std::ptr::null(),
                0,
                0,
                compression,
                Some(if fail_write { fail } else { collect }),
                (&mut output as *mut Vec<u8>).cast(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if compression > 1 || format == 1 || fail_write {
            assert_ne!(result, 0);
            assert_ne!(error[0], 0);
            assert!(output.is_empty());
        } else {
            assert_eq!(result, 0);
            assert_eq!(decode(Format::Xisf, &output).unwrap().pixels, [0.5]);
        }
    }
}
