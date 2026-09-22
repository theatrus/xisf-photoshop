//! Explicit linear CFA conversion. Patterns refer to the stored pixel origin.
use crate::{Image, Result, sample_count};
use seiza_fits::{BayerPattern, FitsImage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CfaInfo {
    /// 0=absent, 1=RGGB, 2=BGGR, 3=GRBG, 4=GBRG, 5=unsupported.
    pub pattern: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub invalid_offsets: bool,
}

impl CfaInfo {
    pub(crate) fn from_image(image: &FitsImage) -> Self {
        if image.planes != 1 {
            return Self::default();
        }
        let pattern = match image.header_str("BAYERPAT") {
            None => 0,
            Some(name) => match BayerPattern::parse(name) {
                Some(BayerPattern::Rggb) => 1,
                Some(BayerPattern::Bggr) => 2,
                Some(BayerPattern::Grbg) => 3,
                Some(BayerPattern::Gbrg) => 4,
                None => 5,
            },
        };
        let offset = |key| match image.header(key) {
            None => Some(0),
            Some(_) => image
                .header_f64(key)
                .filter(|v| v.is_finite() && v.fract() == 0.0)
                .map(|v| v.rem_euclid(2.0) as u32),
        };
        let x = offset("XBAYROFF");
        let y = offset("YBAYROFF");
        Self {
            pattern,
            x_offset: x.unwrap_or(0),
            y_offset: y.unwrap_or(0),
            invalid_offsets: x.is_none() || y.is_none(),
        }
    }
}

/// 0=keep raw; 1=auto from metadata; 2..5=manual RGGB/BGGR/GRBG/GBRG.
/// Auto leaves untagged/unsupported/ordinary mono and existing RGB unchanged.
pub fn apply(image: &mut Image, mode: u32) -> Result<()> {
    if mode > 5 {
        return Err("Invalid debayer mode".into());
    }
    if mode == 0 || image.planes == 3 {
        return Ok(());
    }
    let pattern = if mode == 1 {
        image.cfa.pattern
    } else {
        mode - 1
    };
    let pattern = match pattern {
        1 => BayerPattern::Rggb,
        2 => BayerPattern::Bggr,
        3 => BayerPattern::Grbg,
        4 => BayerPattern::Gbrg,
        _ => return Ok(()),
    };
    if image.cfa.invalid_offsets {
        return Err(
            "Invalid Bayer origin offsets; correct XBAYROFF/YBAYROFF or open as raw grayscale"
                .into(),
        );
    }
    if image.width < 2 || image.height < 2 {
        return Err("Debayering requires at least a 2 by 2 image; open as raw grayscale".into());
    }
    let count = sample_count(image.width, image.height, 3)?;
    let plane = count / 3;
    if image.planes != 1 || image.pixels.len() != plane {
        return Err("Invalid CFA image geometry".into());
    }
    let rgb = seiza_fits::debayer_rgb_f32(
        &image.pixels,
        image.width,
        image.height,
        pattern,
        image.cfa.x_offset as usize,
        image.cfa.y_offset as usize,
    );
    if rgb.data.iter().any(|v| !v.is_finite()) {
        return Err("Debayering produced non-finite samples".into());
    }
    let mut pixels = vec![0.0; count];
    for (i, rgb) in rgb.data.as_chunks::<3>().0.iter().enumerate() {
        for c in 0..3 {
            pixels[c * plane + i] = rgb[c];
        }
    }
    image.pixels = pixels;
    image.planes = 3;
    image.cfa = CfaInfo::default();
    Ok(())
}
