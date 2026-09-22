//! Host-independent astronomy codecs. Photoshop sees planar linear f32 pixels.
//! Float samples retain physical values; unsigned 8/16-bit camera samples use
//! their fixed full-scale range. There is no histogram normalization or stretch.

pub mod debayer;
pub mod ffi;
mod integer_writer;

pub use integer_writer::encode_u16_pixels;

use seiza_fits::{F32ImageData, FitsImage, HeaderValue, Pixels};
use std::io::Write;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Format {
    Fits = 1,
    Xisf = 2,
}

impl TryFrom<u32> for Format {
    type Error = String;
    fn try_from(value: u32) -> Result<Self> {
        match value {
            1 => Ok(Self::Fits),
            2 => Ok(Self::Xisf),
            _ => Err("Unknown astronomy format".into()),
        }
    }
}

impl Format {
    pub fn recognizes(self, bytes: &[u8]) -> bool {
        match self {
            Self::Fits => bytes.starts_with(b"SIMPLE  ="),
            Self::Xisf => bytes.starts_with(b"XISF0100"),
        }
    }
}

#[derive(Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub planes: usize,
    pub pixels: Vec<f32>,
    pub cfa: debayer::CfaInfo,
}

pub fn sample_count(width: usize, height: usize, planes: usize) -> Result<usize> {
    if width == 0 || height == 0 || width > 300_000 || height > 300_000 {
        return Err("Image dimensions must be between 1 and 300000 pixels".into());
    }
    if !matches!(planes, 1 | 3) {
        return Err("Only grayscale and three-channel RGB images are supported".into());
    }
    width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(planes))
        .filter(|&n| n <= isize::MAX as usize / 4)
        .ok_or_else(|| "Image dimensions overflow addressable memory".into())
}

pub fn decode(format: Format, bytes: &[u8]) -> Result<Image> {
    if !format.recognizes(bytes) {
        return Err("File signature does not match the selected format".into());
    }
    let (image, xisf_u32) = match format {
        Format::Fits => (
            FitsImage::from_bytes(bytes).map_err(|e| e.to_string())?,
            false,
        ),
        Format::Xisf => {
            let decoded = seiza_xisf::read_image_from_bytes(bytes, 0).map_err(|e| e.to_string())?;
            let u32_samples = decoded.info.sample_format == seiza_xisf::SampleFormat::UInt32;
            (decoded.image, u32_samples)
        }
    };
    // Seiza can read the first planes of larger FITS cubes. Do not silently
    // present a spectral cube as an RGB image in Photoshop.
    if format == Format::Fits {
        let axis = |key| image.header(key).and_then(HeaderValue::as_i64).unwrap_or(0);
        let naxis = axis("NAXIS");
        if naxis > 3 || (naxis == 3 && !matches!(axis("NAXIS3"), 1 | 3)) {
            return Err("FITS cubes must contain exactly one or three planes".into());
        }
    }
    let (width, height, planes) = (image.width, image.height, image.planes);
    let cfa = debayer::CfaInfo::from_image(&image);
    let count = sample_count(width, height, planes)?;
    let bzero = image.header_f64("BZERO").unwrap_or(0.0);
    let bscale = image.header_f64("BSCALE").unwrap_or(1.0);
    let signed_fits16 = format == Format::Fits
        && matches!(&image.pixels, Pixels::U16(_))
        && (bzero != 32768.0 || bscale != 1.0);
    let divisor = match &image.pixels {
        _ if xisf_u32 => u32::MAX as f32,
        Pixels::U8(_) if bzero == 0.0 && bscale == 1.0 => 255.0,
        Pixels::U16(_) if !signed_fits16 => 65535.0,
        _ => 1.0,
    };
    let mut pixels = image.into_physical_f32();
    if pixels.len() != count {
        return Err("Decoded sample count does not match the image geometry".into());
    }
    if signed_fits16 {
        // seiza-fits 0.2.2 clamps negative unscaled i16 samples on its camera
        // fast path. Restore these from the validated payload to preserve signed
        // scientific data; the upstream reader still owns all FITS parsing.
        let end_card = bytes
            .as_chunks::<80>()
            .0
            .iter()
            .position(|card| &card[..8] == b"END     ")
            .ok_or("Missing FITS END card")?;
        let offset = (end_card + 1).div_ceil(36) * 2880;
        let raw = bytes
            .get(offset..offset + count * 2)
            .ok_or("Truncated FITS pixels")?;
        for (pixel, pair) in pixels.iter_mut().zip(raw.as_chunks::<2>().0) {
            *pixel = (bzero + bscale * f64::from(i16::from_be_bytes([pair[0], pair[1]]))) as f32;
        }
    }
    if divisor != 1.0 {
        pixels.iter_mut().for_each(|v| *v /= divisor);
    }
    // Photoshop cannot meaningfully edit NaNs or infinities. Reject explicitly
    // rather than silently replacing scientific samples with black.
    if pixels.iter().any(|v| !v.is_finite()) {
        return Err(
            "Image contains NaN or infinite samples; replace them before opening in Photoshop"
                .into(),
        );
    }
    Ok(Image {
        width,
        height,
        planes,
        pixels,
        cfa,
    })
}

pub fn encode(format: Format, image: &Image, writer: impl Write) -> Result<()> {
    encode_pixels(
        format,
        image.width,
        image.height,
        image.planes,
        &image.pixels,
        writer,
    )
}

pub fn encode_pixels(
    format: Format,
    width: usize,
    height: usize,
    planes: usize,
    pixels: &[f32],
    writer: impl Write,
) -> Result<()> {
    if pixels.len() != sample_count(width, height, planes)? {
        return Err("Sample count does not match the image geometry".into());
    }
    if pixels.iter().any(|v| !v.is_finite()) {
        return Err("Cannot save NaN or infinite samples".into());
    }
    let data = if planes == 1 {
        F32ImageData::Mono(pixels)
    } else {
        F32ImageData::RgbPlanar(pixels)
    };
    match format {
        Format::Fits => seiza_fits::write_f32_image_to(writer, width, height, data, &[])
            .map_err(|e| e.to_string()),
        Format::Xisf => seiza_xisf::write_f32_image_to(writer, width, height, data, &[])
            .map_err(|e| e.to_string()),
    }
}
