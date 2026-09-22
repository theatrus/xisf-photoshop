//! Generate small, asymmetric host smoke-test images without external data.
use seiza_photoshop::{Format, Image, encode};
use std::{
    fs::{self, File},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .unwrap_or_else(|| "build/fixtures".into()),
    );
    fs::create_dir_all(&directory)?;
    let (width, height) = (128, 96);
    for planes in [1, 3] {
        let mut pixels = Vec::new();
        for plane in 0..planes {
            for y in 0..height {
                for x in 0..width {
                    pixels.push(match plane {
                        0 => x as f32 / (width - 1) as f32,
                        1 => y as f32 / (height - 1) as f32,
                        _ => {
                            if x < width / 2 && y < height / 2 {
                                1.0
                            } else {
                                0.0
                            }
                        }
                    });
                }
            }
        }
        let image = Image {
            width,
            height,
            planes,
            pixels,
        };
        for (format, extension) in [(Format::Fits, "fits"), (Format::Xisf, "xisf")] {
            let path = directory.join(format!(
                "{}.{extension}",
                if planes == 1 { "mono" } else { "rgb" }
            ));
            encode(format, &image, File::create(&path)?)?;
            println!("{}", path.display());
        }
    }
    Ok(())
}
