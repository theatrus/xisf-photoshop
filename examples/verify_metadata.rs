//! Exercise a real XISF through decode, document XMP, and a self-contained save.
//! Usage: cargo run --release --example verify_metadata -- input.xisf output.xisf
//! Output must not already exist; source data is never modified.
use seiza_photoshop::{
    Format, decode,
    metadata::{self, Metadata},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::BufWriter,
};

#[derive(Debug, PartialEq)]
struct Attachment {
    attributes: Vec<(String, String)>,
    digest: Vec<u8>,
}

fn attachments(bytes: &[u8]) -> Vec<Attachment> {
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let xml = std::str::from_utf8(&bytes[16..16 + length]).unwrap();
    let doc = roxmltree::Document::parse(xml).unwrap();
    doc.descendants()
        .filter(|n| !matches!(n.tag_name().name(), "Image" | "Thumbnail"))
        .filter_map(|n| {
            let location = n.attribute("location")?.strip_prefix("attachment:")?;
            let (start, size) = location.split_once(':').unwrap();
            let start: usize = start.parse().unwrap();
            let size: usize = size.parse().unwrap();
            let mut attributes: Vec<_> = n
                .attributes()
                .filter(|a| a.name() != "location")
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect();
            attributes.sort();
            Some(Attachment {
                attributes,
                digest: Sha256::digest(&bytes[start..start + size]).to_vec(),
            })
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        return Err("Usage: verify_metadata input.xisf output.xisf".into());
    }
    let source = fs::read(&args[1])?;
    let expected_attachments = attachments(&source);
    let image = decode(Format::Xisf, &source)?;
    drop(source);
    println!(
        "Decoded {} x {} x {}",
        image.width, image.height, image.planes
    );
    let packet = image.metadata.xmp()?;
    println!("Document XMP: {} bytes", packet.len());
    let metadata = Metadata::from_xmp(&packet)?.ok_or("No metadata in XMP")?;
    // Include the same ICC extraction performed by the native Open adapter.
    let expected_profile = metadata.icc_profile(image.planes)?;
    println!("ICC profile: {} bytes", expected_profile.len());
    let mut output = BufWriter::new(File::create_new(&args[2])?);
    metadata::encode(
        Format::Xisf,
        32,
        image.width,
        image.height,
        image.planes,
        &image.pixels,
        &metadata,
        &mut output,
    )?;
    use std::io::Write;
    output.flush()?;
    drop(output);
    let pixel_hash = |pixels: &[f32]| {
        let mut hash = Sha256::new();
        for pixel in pixels {
            hash.update(pixel.to_le_bytes());
        }
        hash.finalize()
    };
    let expected_pixels = pixel_hash(&image.pixels);
    drop(image);
    let saved = fs::read(&args[2])?;
    assert_eq!(attachments(&saved), expected_attachments);
    println!(
        "Verified {} metadata attachments and their storage attributes",
        expected_attachments.len()
    );
    let reopened = decode(Format::Xisf, &saved)?;
    assert_eq!(pixel_hash(&reopened.pixels), expected_pixels);
    assert_eq!(reopened.metadata.cards, metadata.cards);
    assert_eq!(
        reopened.metadata.icc_profile(reopened.planes)?,
        expected_profile
    );
    let reopened_packet = reopened.metadata.xmp()?;
    println!(
        "Saved and reopened; pixels verified. Metadata XMP: {} bytes",
        reopened_packet.len()
    );
    Ok(())
}
