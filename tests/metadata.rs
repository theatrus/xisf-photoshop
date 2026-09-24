use base64::{Engine, engine::general_purpose::STANDARD as B64};
use seiza_photoshop::{
    Format, decode, encode_pixels,
    metadata::{self, Metadata},
};

fn fits_fixture() -> (Vec<u8>, Vec<String>) {
    let mut base = Vec::new();
    encode_pixels(Format::Fits, 2, 2, 1, &[0.0, 0.25, 0.5, 1.0], &mut base).unwrap();
    let end = base
        .as_chunks::<80>()
        .0
        .iter()
        .position(|c| c.starts_with(b"END     "))
        .unwrap()
        * 80;
    let cards: Vec<String> = [
        "OBJECT  = 'M 31 / Andromeda' / target name",
        "EXPTIME =                  180 / seconds",
        "FILTER  = 'Ha' / filter",
        "OBSERVER= 'O''Brien' / escaped quote",
        "COMMENT first comment",
        "COMMENT second comment",
        "HISTORY captured by camera",
        "HISTORY calibrated before Photoshop",
        "HIERARCH INSTRUMENT DETECTOR GAIN = 100 / long keyword",
        "LONGSTR = 'This is the start of a long value &'",
        "CONTINUE  'and its continuation'",
        "CRPIX1  =                  1.5",
        "CRPIX2  =                  1.5",
        "BAYERPAT= 'RGGB'",
        "XBAYROFF=                    0",
        "CHECKSUM= 'obsolete checksum'",
        "DATAMIN =                 -999",
    ]
    .into_iter()
    .map(|s| format!("{s:80}"))
    .collect();
    let mut data = base[..end].to_vec();
    for card in &cards {
        data.extend(card.as_bytes());
    }
    data.extend(format!("{:80}", "END").as_bytes());
    data.resize(data.len().div_ceil(2880) * 2880, b' ');
    data.extend(&base[2880..]);
    (data, cards[..15].to_vec())
}

fn xisf_fixture() -> (Vec<u8>, Vec<u8>) {
    use std::io::Write;
    let mut compressed =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    compressed
        .write_all(&[0, 255, 13, 10, 128, 1, 2, 3])
        .unwrap();
    let block = compressed.finish().unwrap();
    let xml = format!(
        r#"<?xml version="1.0"?><xisf version="1.0" xmlns="http://www.pixinsight.com/xisf">
      <Image id="master" geometry="2:2:1" sampleFormat="Float32" colorSpace="Gray" location="attachment:4096:16">
        <FITSKeyword name="OBJECT" value="'M 31'" comment="target &amp; field"/>
        <FITSKeyword name="EXPTIME" value="180" comment="seconds"/>
        <FITSKeyword name="HISTORY" value="first processing step"/>
        <FITSKeyword name="HISTORY" value="second processing step"/>
        <FITSKeyword name="BAYERPAT" value="'RGGB'"/>
        <ColorFilterArray pattern="RGGB" width="2" height="2"/>
        <Property id="Observer:Name" type="String">星 &amp; O'Brien &lt;test&gt;</Property>
        <Property id="Instrument:Gain" type="Float64" value="123.456789012345" comment="original gain"/>
        <Property id="Private:Vector" type="UI8Vector" length="8" compression="zlib:8" location="attachment:4112:{}"/>
        <Property id="Private:Inline" type="String" location="inline:base64">aGVsbG8=</Property>
        <Property id="PCL:AstrometricSolution:ReferenceImageCoordinates" type="String">1.5,1.5</Property>
        <Property id="AstrometricSolution:ReferenceCelestialCoordinates" type="String">83,22</Property>
        <FITSKeyword name="CRPIX1" value="1.5"/>
        <Resolution horizontal="300" vertical="300" unit="inch"/>
        <RGBWorkingSpace gamma="2.2" srgbGamma="true"/>
        <Thumbnail geometry="1:1:1" sampleFormat="UInt8" colorSpace="Gray" location="inline:base64">AA==</Thumbnail>
      </Image>
      <Image id="ignored" geometry="1:1:1" sampleFormat="Float32" colorSpace="Gray" location="attachment:4096:4"><Property id="OtherImage" type="String">not imported</Property></Image>
      <Metadata><Property id="Custom:FileProperty" type="String">file-level metadata</Property></Metadata>
    </xisf>"#,
        block.len()
    );
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend((xml.len() as u32).to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(xml.as_bytes());
    bytes.resize(4096, 0);
    for value in [0.0f32, 0.25, 0.5, 1.0] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(&block);
    (bytes, block)
}

fn save(format: Format, depth: u32, image: &seiza_photoshop::Image) -> Vec<u8> {
    // Emulate a PSD round trip / Save As: only serialized document XMP survives.
    let xmp = image.metadata.xmp().unwrap();
    let metadata = Metadata::from_xmp(&xmp).unwrap().unwrap();
    let mut out = Vec::new();
    metadata::encode(
        format,
        depth,
        image.width,
        image.height,
        image.planes,
        &image.pixels,
        &metadata,
        &mut out,
    )
    .unwrap();
    out
}

fn xml(bytes: &[u8]) -> &str {
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    std::str::from_utf8(&bytes[16..16 + length]).unwrap()
}

#[test]
fn fits_cards_survive_saves_and_common_keywords_survive_format_switches() {
    let (original, cards) = fits_fixture();
    let first = decode(Format::Fits, &original).unwrap();
    assert_eq!(first.metadata.cards, cards);
    for depth in [16, 32] {
        let fits = save(Format::Fits, depth, &first);
        let reopened = decode(Format::Fits, &fits).unwrap();
        assert_eq!(reopened.metadata.cards, cards);
        let fits = save(Format::Fits, depth, &reopened);
        assert_eq!(decode(Format::Fits, &fits).unwrap().metadata.cards, cards);
        let xisf = save(Format::Xisf, depth, &reopened);
        let reopened = decode(Format::Xisf, &xisf).unwrap();
        assert!(
            reopened
                .metadata
                .cards
                .iter()
                .any(|c| c.starts_with("EXPTIME") && c.contains("180"))
        );
        let header = String::from_utf8_lossy(&fits[..2880]);
        assert!(!header.contains("obsolete checksum"));
        assert!(!header.contains("DATAMIN"));
        assert_eq!(header.matches("NAXIS1").count(), 1);
        if depth == 16 {
            assert!(header.contains("BZERO   =                32768"));
        } else {
            assert!(!header.contains("BZERO"));
        }
    }
}

#[test]
fn xisf_properties_scopes_and_binary_attachments_survive_save_as() {
    let (original, block) = xisf_fixture();
    let mut image = decode(Format::Xisf, &original).unwrap();
    for depth in [32, 16] {
        for format in [Format::Xisf, Format::Fits, Format::Fits, Format::Xisf] {
            let bytes = save(format, depth, &image);
            if format == Format::Xisf {
                let doc = roxmltree::Document::parse(xml(&bytes)).unwrap();
                let find = |id| {
                    doc.descendants()
                        .find(|n| n.attribute("id") == Some(id))
                        .unwrap()
                };
                assert_eq!(find("Observer:Name").text(), Some("星 & O'Brien <test>"));
                assert_eq!(
                    find("Instrument:Gain").attribute("value"),
                    Some("123.456789012345")
                );
                assert_eq!(find("Private:Inline").text(), Some("aGVsbG8="));
                assert_eq!(
                    find("Private:Vector").attribute("compression"),
                    Some("zlib:8")
                );
                assert_eq!(
                    find("Custom:FileProperty")
                        .parent()
                        .unwrap()
                        .tag_name()
                        .name(),
                    "Metadata"
                );
                assert_eq!(
                    doc.descendants()
                        .filter(|n| n.has_tag_name(("http://www.pixinsight.com/xisf", "Image")))
                        .count(),
                    1
                );
                assert!(!xml(&bytes).contains("OtherImage"));
                assert!(!xml(&bytes).contains("Thumbnail"));
                let loc: Vec<_> = find("Private:Vector")
                    .attribute("location")
                    .unwrap()
                    .split(':')
                    .collect();
                let start: usize = loc[1].parse().unwrap();
                let length: usize = loc[2].parse().unwrap();
                assert_eq!(&bytes[start..start + length], block);
                assert_eq!(start % 4096, 0);
            } else {
                assert!(seiza_fits::FitsImage::from_bytes(&bytes).is_ok());
                let converted = decode(format, &bytes).unwrap();
                assert!(
                    converted
                        .metadata
                        .cards
                        .iter()
                        .any(|c| c.starts_with("EXPTIME"))
                );
                // Saving a FITS copy does not mutate the document's source XISF metadata.
                continue;
            }
            image = decode(format, &bytes).unwrap();
        }
    }
}

#[test]
fn debayer_and_resize_remove_stale_cfa_and_known_wcs_but_keep_acquisition() {
    for (format, bytes) in [
        (Format::Fits, fits_fixture().0),
        (Format::Xisf, xisf_fixture().0),
    ] {
        let mut image = decode(format, &bytes).unwrap();
        seiza_photoshop::debayer::apply(&mut image, 1).unwrap();
        for output in [Format::Fits, Format::Xisf] {
            let reopened = decode(output, &save(output, 32, &image)).unwrap();
            assert_eq!(reopened.cfa.pattern, 0);
            assert!(
                reopened
                    .metadata
                    .cards
                    .iter()
                    .any(|s| s.starts_with("EXPTIME"))
            );
        }
        image.width = 1;
        image.height = 4;
        let output = save(Format::Xisf, 32, &image);
        assert!(!xml(&output).contains("AstrometricSolution:"));
        let reopened = decode(Format::Xisf, &output).unwrap();
        assert!(
            !reopened
                .metadata
                .cards
                .iter()
                .any(|s| s.starts_with("CRPIX"))
        );
    }
}

#[test]
fn xmp_accepts_photoshop_attribute_serialization_and_rejects_corruption() {
    let image = decode(Format::Fits, &fits_fixture().0).unwrap();
    let xmp = String::from_utf8(image.metadata.xmp().unwrap()).unwrap();
    let doc = roxmltree::Document::parse(&xmp).unwrap();
    let payload = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Metadata")
        .unwrap()
        .text()
        .unwrap();
    let attribute = format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:custom="https://seiza.fyi/ns/photoshop/1.0/" custom:Metadata="{payload}"/></rdf:RDF></x:xmpmeta>"#
    );
    assert_eq!(
        Metadata::from_xmp(attribute.as_bytes())
            .unwrap()
            .unwrap()
            .cards,
        image.metadata.cards
    );
    assert!(Metadata::from_xmp(attribute.replace(payload, "broken!").as_bytes()).is_err());
    assert!(Metadata::from_xmp(b"<xmp/>").unwrap().is_none());
    let mut value: serde_json::Value =
        serde_json::from_slice(&B64.decode(payload).unwrap()).unwrap();
    value["version"] = 99.into();
    assert!(
        Metadata::from_xmp(
            attribute
                .replace(payload, &B64.encode(serde_json::to_vec(&value).unwrap()))
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn explicit_astrometry_removal_at_unchanged_dimensions_preserves_acquisition_and_pixels() {
    use seiza_photoshop::ffi;
    unsafe extern "C" fn append(
        context: *mut std::ffi::c_void,
        bytes: *const u8,
        size: usize,
    ) -> i32 {
        unsafe {
            (&mut *context.cast::<Vec<u8>>())
                .extend_from_slice(std::slice::from_raw_parts(bytes, size));
        }
        0
    }
    for (format, bytes) in [
        (Format::Fits, fits_fixture().0),
        (Format::Xisf, xisf_fixture().0),
    ] {
        let image = decode(format, &bytes).unwrap();
        let xmp = image.metadata.xmp().unwrap();
        for output_format in [Format::Fits, Format::Xisf] {
            for remove in [0, 1, 2] {
                let mut out = Vec::new();
                let mut error = [0i8; 1024];
                let status = unsafe {
                    ffi::seiza_encode_with_options(
                        output_format as u32,
                        32,
                        2,
                        2,
                        1,
                        image.pixels.as_ptr(),
                        image.pixels.len(),
                        xmp.as_ptr(),
                        xmp.len(),
                        0,
                        std::ptr::null(),
                        0,
                        remove,
                        Some(append),
                        (&mut out as *mut Vec<u8>).cast(),
                        error.as_mut_ptr(),
                        error.len(),
                    )
                };
                if remove == 2 {
                    assert_ne!(status, 0);
                    assert!(out.is_empty());
                    continue;
                }
                assert_eq!(status, 0);
                let reopened = decode(output_format, &out).unwrap();
                assert_eq!(reopened.pixels, image.pixels);
                assert!(
                    reopened
                        .metadata
                        .cards
                        .iter()
                        .any(|c| c.starts_with("EXPTIME"))
                );
                assert_eq!(
                    reopened
                        .metadata
                        .cards
                        .iter()
                        .any(|c| c.starts_with("CRPIX1")),
                    remove == 0
                );
                assert!(
                    reopened
                        .metadata
                        .cards
                        .iter()
                        .any(|c| c.starts_with("BAYERPAT")),
                    "Removing coordinates alone must not remove CFA metadata"
                );
                if format == Format::Xisf && output_format == Format::Xisf {
                    assert_eq!(xml(&out).contains("PCL:AstrometricSolution:"), remove == 0);
                    assert_eq!(xml(&out).contains("id=\"AstrometricSolution:"), remove == 0);
                    assert!(xml(&out).contains("Observer:Name"));
                    assert!(xml(&out).contains("Private:Vector"));
                }
                assert_eq!(
                    image.metadata.xmp().unwrap(),
                    xmp,
                    "Save must not mutate document metadata"
                );
            }
        }
    }
}

#[test]
fn missing_binary_metadata_fails_instead_of_silently_dropping_it() {
    let (mut bytes, _) = xisf_fixture();
    bytes.truncate(bytes.len() - 1);
    assert!(
        decode(Format::Xisf, &bytes)
            .unwrap_err()
            .contains("Truncated metadata attachment")
    );
}

#[test]
fn growing_xml_relocates_pixels_and_attachments_without_corrupting_either() {
    let (original, block) = xisf_fixture();
    let original = decode(Format::Xisf, &original).unwrap();
    let packet = String::from_utf8(original.metadata.xmp().unwrap()).unwrap();
    let doc = roxmltree::Document::parse(&packet).unwrap();
    let payload = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Metadata")
        .unwrap()
        .text()
        .unwrap();
    let mut data: serde_json::Value =
        serde_json::from_slice(&B64.decode(payload).unwrap()).unwrap();
    let large = format!(
        "<Property id=\"Private:Long\" type=\"String\">{}</Property></Image>",
        "é &amp; text ".repeat(1500)
    );
    data["xml"] = data["xml"]
        .as_str()
        .unwrap()
        .replace("</Image>", &large)
        .into();
    let packet = packet.replace(payload, &B64.encode(serde_json::to_vec(&data).unwrap()));
    let metadata = Metadata::from_xmp(packet.as_bytes()).unwrap().unwrap();
    let mut out = Vec::new();
    metadata::encode(
        Format::Xisf,
        32,
        2,
        2,
        1,
        &original.pixels,
        &metadata,
        &mut out,
    )
    .unwrap();
    let doc = roxmltree::Document::parse(xml(&out)).unwrap();
    let loc = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Image")
        .unwrap()
        .attribute("location")
        .unwrap();
    let start = loc.split(':').nth(1).unwrap().parse::<usize>().unwrap();
    assert!(start > 4096 && start.is_multiple_of(4096));
    let loc = doc
        .descendants()
        .find(|n| n.attribute("id") == Some("Private:Vector"))
        .unwrap()
        .attribute("location")
        .unwrap();
    let start = loc.split(':').nth(1).unwrap().parse::<usize>().unwrap();
    assert_eq!(&out[start..start + block.len()], block);
    assert_eq!(decode(Format::Xisf, &out).unwrap().pixels, original.pixels);
}

#[test]
fn ffi_metadata_failures_are_errors_before_any_output_is_written() {
    use seiza_photoshop::ffi;
    unsafe extern "C" fn fail(_: *mut std::ffi::c_void, _: *const u8, _: usize) -> i32 {
        1
    }
    let image = decode(Format::Fits, &fits_fixture().0).unwrap();
    let mut error = [0i8; 1024];
    unsafe {
        assert_ne!(
            ffi::seiza_image_xmp(
                &image,
                Some(fail),
                std::ptr::null_mut(),
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
        assert_ne!(
            ffi::seiza_image_xmp(
                std::ptr::null(),
                Some(fail),
                std::ptr::null_mut(),
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
        assert_ne!(
            ffi::seiza_encode_with_metadata(
                1,
                32,
                2,
                2,
                1,
                image.pixels.as_ptr(),
                4,
                std::ptr::null(),
                1,
                Some(fail),
                std::ptr::null_mut(),
                error.as_mut_ptr(),
                error.len()
            ),
            0
        );
    }
}
