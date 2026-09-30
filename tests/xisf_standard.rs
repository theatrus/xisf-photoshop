use base64::{Engine, engine::general_purpose::STANDARD as B64};
use seiza_photoshop::{
    Format, decode,
    metadata::{self, Metadata},
};
use sha3::Digest;

fn fixture(xml: &str, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend((xml.len() as u32).to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(xml.as_bytes());
    assert!(bytes.len() <= 4096);
    bytes.resize(4096, 0);
    bytes.extend(payload);
    bytes
}

fn saved(image: &seiza_photoshop::Image, depth: u32) -> Vec<u8> {
    let metadata = Metadata::from_xmp(&image.metadata.xmp().unwrap())
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    metadata::encode(
        Format::Xisf,
        depth,
        image.width,
        image.height,
        image.planes,
        &image.pixels,
        &metadata,
        &mut bytes,
    )
    .unwrap();
    bytes
}

fn xml(bytes: &[u8]) -> &str {
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    std::str::from_utf8(&bytes[16..16 + length]).unwrap()
}

#[test]
fn uint64_uses_full_scale_even_with_stale_fits_bitpix() {
    for order in ["little", "big"] {
        let raw: Vec<_> = [0u64, 1 << 63, u64::MAX]
            .into_iter()
            .flat_map(|n| {
                if order == "little" {
                    n.to_le_bytes()
                } else {
                    n.to_be_bytes()
                }
            })
            .collect();
        let bytes = fixture(
            &format!(
                r#"<xisf version="1.0"><Image geometry="3:1:1" sampleFormat="UInt64" byteOrder="{order}" location="attachment:4096:24"><FITSKeyword name="BITPIX" value="-64"/></Image></xisf>"#
            ),
            &raw,
        );
        let image = decode(Format::Xisf, &bytes).unwrap();
        assert_eq!(image.pixels, [0.0, 0.5, 1.0]);
        for depth in [16, 32] {
            let reopened = decode(Format::Xisf, &saved(&image, depth)).unwrap();
            for (actual, expected) in reopened.pixels.iter().zip(&image.pixels) {
                assert!((actual - expected).abs() < 1.0 / 65535.0);
            }
        }
    }
}

#[test]
fn inline_and_embedded_interleaved_pixels_save_as_planar_without_old_payload() {
    let raw: Vec<_> = [0.0f32, 0.25, 0.75, 0.125, 0.5, 1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let encoded = B64.encode(&raw);
    for (location, body) in [
        ("inline:base64", encoded.clone()),
        (
            "embedded",
            format!(r#"<Data encoding="base64">{encoded}</Data>"#),
        ),
    ] {
        let bytes = fixture(
            &format!(
                r#"<xisf version="1.0"><Image geometry="2:1:3" sampleFormat="Float32" bounds="0:1" colorSpace="RGB" pixelStorage="Normal" location="{location}">{body}<Property id="Observer:Name" type="String">Test observer</Property></Image></xisf>"#
            ),
            &[],
        );
        let image = decode(Format::Xisf, &bytes).unwrap();
        assert_eq!(image.pixels, [0.0, 0.125, 0.25, 0.5, 0.75, 1.0]);
        let out = saved(&image, 32);
        assert_eq!(decode(Format::Xisf, &out).unwrap().pixels, image.pixels);
        assert!(xml(&out).contains("Test observer"));
        assert!(!xml(&out).contains(&encoded));
        assert!(!xml(&out).contains("<Data"));
        assert!(xml(&out).contains("pixelStorage=\"Planar\""));
    }
}

#[test]
fn cielab_neutral_samples_convert_to_rgb_and_save_as_rgb() {
    // Neutral Lab at black and white, normalized a=b=0.5.
    let raw: Vec<_> = [0.0f32, 1.0, 0.5, 0.5, 0.5, 0.5]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let bytes = fixture(
        r#"<xisf version="1.0"><Image geometry="2:1:3" sampleFormat="Float32" bounds="0:1" colorSpace="CIELab" location="attachment:4096:24"/></xisf>"#,
        &raw,
    );
    let image = decode(Format::Xisf, &bytes).unwrap();
    for (actual, expected) in image.pixels.iter().zip([0.0, 1.0, 0.0, 1.0, 0.0, 1.0]) {
        assert!((actual - expected).abs() < 0.001);
    }
    let out = saved(&image, 32);
    assert!(xml(&out).contains("colorSpace=\"RGB\""));
    assert!(!xml(&out).contains("CIELab"));
    assert_eq!(decode(Format::Xisf, &out).unwrap().pixels, image.pixels);
}

#[test]
fn sha3_pixel_checksums_are_verified() {
    let raw = [0u8, 127, 255];
    for (name, hash) in [
        ("sha3-256", sha3::Sha3_256::digest(raw).to_vec()),
        ("sha3-512", sha3::Sha3_512::digest(raw).to_vec()),
    ] {
        let hex: String = hash.iter().map(|v| format!("{v:02x}")).collect();
        let xml = format!(
            r#"<xisf version="1.0"><Image geometry="3:1:1" sampleFormat="UInt8" location="attachment:4096:3" checksum="{name}:{hex}"/></xisf>"#
        );
        let image = decode(Format::Xisf, &fixture(&xml, &raw)).unwrap();
        assert_eq!(image.pixels, [0.0, 127.0 / 255.0, 1.0]);
        assert!(decode(Format::Xisf, &fixture(&xml, &[1, 127, 255])).is_err());
    }
}

#[test]
fn referenced_metadata_in_second_image_survives_and_is_cleaned_after_crop() {
    let bytes = fixture(
        r#"<xisf version="1.0"><Image geometry="2:1:1" sampleFormat="UInt8" location="attachment:4096:2" uuid="old-source" offset="0">
      <Reference ref="object"/><Reference ref="solution"/><Reference ref="profile"/>
    </Image><Image geometry="1:1:1" sampleFormat="UInt8" location="attachment:4096:1">
      <FITSKeyword uid="object" name="OBJECT" value="'M 42'"/>
      <Property uid="solution" id="AstrometricSolution:Test" type="String">old coordinates</Property>
      <ICCProfile uid="profile" location="attachment:4098:132"/>
    </Image></xisf>"#,
        &{
            let mut data = vec![0, 255];
            let mut icc = vec![0; 132];
            icc[..4].copy_from_slice(&132u32.to_be_bytes());
            icc[16..20].copy_from_slice(b"GRAY");
            icc[36..40].copy_from_slice(b"acsp");
            data.extend(icc);
            data
        },
    );
    let mut image = decode(Format::Xisf, &bytes).unwrap();
    let profile = image.metadata.icc_profile(1).unwrap();
    assert_eq!(profile.len(), 132);
    assert!(image.metadata.cards.iter().any(|c| c.contains("M 42")));
    for depth in [16, 32] {
        let out = saved(&image, depth);
        assert!(!xml(&out).contains("Reference"));
        assert!(!xml(&out).contains("old-source"));
        assert!(!xml(&out).contains("offset="));
        assert_eq!(xml(&out).matches("<Image ").count(), 1);
        assert!(xml(&out).contains("old coordinates"));
        assert_eq!(
            decode(Format::Xisf, &out)
                .unwrap()
                .metadata
                .icc_profile(1)
                .unwrap(),
            profile
        );
    }
    image.width = 1;
    image.pixels.truncate(1);
    let out = saved(&image, 32);
    assert!(!xml(&out).contains("AstrometricSolution:"));
    assert!(xml(&out).contains("M 42"));
}

#[test]
fn complex_pixels_are_rejected_explicitly() {
    let bytes = fixture(
        r#"<xisf version="1.0"><Image geometry="1:1:1" sampleFormat="Complex32" location="attachment:4096:8"/></xisf>"#,
        &[0; 8],
    );
    assert!(
        decode(Format::Xisf, &bytes)
            .unwrap_err()
            .to_lowercase()
            .contains("complex")
    );
}
