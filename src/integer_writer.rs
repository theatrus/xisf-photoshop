//! UInt16 output for the sample type not yet exposed by Seiza's writer APIs.
//! Inputs are normalized linear samples. The caller obtains consent to quantize
//! and clip before calling; validation completes before any output is written.
use crate::{Format, Result, sample_count};
use std::io::Write;

pub fn encode_u16_pixels(
    format: Format,
    width: usize,
    height: usize,
    planes: usize,
    pixels: &[f32],
    mut writer: impl Write,
) -> Result<()> {
    if pixels.len() != sample_count(width, height, planes)? {
        return Err("Sample count does not match the image geometry".into());
    }
    if pixels.iter().any(|v| !v.is_finite()) {
        return Err("Cannot save NaN or infinite samples".into());
    }
    let write = |w: &mut dyn Write, bytes: &[u8]| w.write_all(bytes).map_err(|e| e.to_string());
    match format {
        Format::Fits => {
            // FITS unsigned convention: signed big-endian storage + BZERO.
            let mut cards = vec![
                "SIMPLE  =                    T".to_owned(),
                "BITPIX  =                   16".to_owned(),
                format!("NAXIS   = {:20}", if planes == 1 { 2 } else { 3 }),
                format!("NAXIS1  = {width:20}"),
                format!("NAXIS2  = {height:20}"),
            ];
            if planes == 3 {
                cards.push("NAXIS3  =                    3".to_owned());
            }
            cards.extend([
                "BSCALE  =                    1".to_owned(),
                "BZERO   =                32768".to_owned(),
                "END".to_owned(),
            ]);
            let mut header: Vec<u8> = cards
                .iter()
                .flat_map(|c| format!("{c:80}").into_bytes())
                .collect();
            header.resize(header.len().div_ceil(2880) * 2880, b' ');
            write(&mut writer, &header)?;
        }
        Format::Xisf => {
            // Fixed numeric-only XML always fits in the first 4096-byte block.
            let color = if planes == 1 { "Gray" } else { "RGB" };
            let size = pixels.len() * 2;
            let xml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><xisf version=\"1.0\" \
                 xmlns=\"http://www.pixinsight.com/xisf\"><Image geometry=\"{width}:{height}:{planes}\" \
                 sampleFormat=\"UInt16\" colorSpace=\"{color}\" pixelStorage=\"Planar\" \
                 byteOrder=\"little\" location=\"attachment:4096:{size}\"/></xisf>"
            );
            let mut header = b"XISF0100".to_vec();
            header.extend_from_slice(&4080u32.to_le_bytes());
            header.extend_from_slice(&[0; 4]);
            header.extend_from_slice(xml.as_bytes());
            header.resize(4096, b' ');
            write(&mut writer, &header)?;
        }
    }
    let mut buffer = Vec::with_capacity(65536);
    for chunk in pixels.chunks(32768) {
        buffer.clear();
        for &pixel in chunk {
            // f64 arithmetic avoids extra f32 rounding near half-code boundaries.
            let value = (f64::from(pixel).clamp(0.0, 1.0) * 65535.0).round() as u16;
            let bytes = match format {
                Format::Fits => (value ^ 0x8000).to_be_bytes(),
                Format::Xisf => value.to_le_bytes(),
            };
            buffer.extend_from_slice(&bytes);
        }
        write(&mut writer, &buffer)?;
    }
    if format == Format::Fits {
        let padding = (2880 - pixels.len() * 2 % 2880) % 2880;
        write(&mut writer, &vec![0; padding])?;
    }
    Ok(())
}
