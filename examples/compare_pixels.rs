//! Read-only pixel/profile comparison for real Photoshop round-trip diagnostics.
//! Usage: cargo run --release --example compare_pixels -- reference.xisf saved.xisf [...]
//! Reports the full image and pixels outside the test edit rectangle [100,164)^2.
use seiza_photoshop::{Format, Image, decode};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn read(path: &Path) -> Result<Image, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let format = if Format::Xisf.recognizes(&bytes) {
        Format::Xisf
    } else {
        Format::Fits
    };
    Ok(decode(format, &bytes)?)
}

fn profile(image: &Image) -> Result<serde_json::Value, String> {
    let bytes = image.metadata.icc_profile(image.planes)?;
    if bytes.is_empty() {
        return Ok(serde_json::json!({"bytes": 0}));
    }
    let number =
        |offset| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    let mut tags = Vec::new();
    for i in 0..number(128) {
        let entry = 132 + 12 * i;
        let signature = String::from_utf8_lossy(&bytes[entry..entry + 4]);
        if matches!(
            signature.as_ref(),
            "desc" | "kTRC" | "rTRC" | "gTRC" | "bTRC"
        ) {
            let offset = number(entry + 4);
            let length = number(entry + 8);
            let block = &bytes[offset..offset + length];
            tags.push(serde_json::json!({
                "tag": signature,
                "length": length,
                "prefix_hex": block.iter().take(48).map(|b| format!("{b:02x}")).collect::<String>(),
                "text": String::from_utf8_lossy(&block[..block.len().min(160)]).replace('\0', "")
            }));
        }
    }
    Ok(
        serde_json::json!({"bytes": bytes.len(), "sha256": Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>(), "tags": tags}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 {
        return Err("Usage: compare_pixels reference.xisf saved.xisf [...]".into());
    }
    let reference = read(Path::new(&args[0]))?;
    println!(
        "{}",
        serde_json::json!({"reference": args[0].to_string_lossy(), "profile": profile(&reference)?})
    );
    for path in &args[1..] {
        let saved = read(Path::new(path))?;
        if (reference.width, reference.height, reference.planes)
            != (saved.width, saved.height, saved.planes)
        {
            return Err(
                "Image dimensions/channels differ; align coordinates before comparing".into(),
            );
        }
        let mut exact = 0usize;
        let mut outside_changed = 0usize;
        let mut max_difference = 0.0f64;
        let mut outside_max_difference = 0.0f64;
        let mut sum_square_difference = 0.0;
        let mut examples = Vec::new();
        let mut range = [f32::INFINITY, f32::NEG_INFINITY];
        let mut original_range = range;
        for (i, (&a, &b)) in reference.pixels.iter().zip(&saved.pixels).enumerate() {
            original_range[0] = original_range[0].min(a);
            original_range[1] = original_range[1].max(a);
            range[0] = range[0].min(b);
            range[1] = range[1].max(b);
            let difference = (f64::from(b) - f64::from(a)).abs();
            max_difference = max_difference.max(difference);
            sum_square_difference += difference * difference;
            if a.to_bits() == b.to_bits() {
                exact += 1;
            } else {
                let x = i % reference.width;
                let y = (i / reference.width) % reference.height;
                if !(100..164).contains(&x) || !(100..164).contains(&y) {
                    outside_changed += 1;
                    outside_max_difference = outside_max_difference.max(difference);
                    if examples.len() < 8 {
                        examples
                            .push(serde_json::json!({"x": x, "y": y, "reference": a, "saved": b}));
                    }
                }
            }
        }
        println!(
            "{}",
            serde_json::json!({
                "saved": path.to_string_lossy(), "samples": reference.pixels.len(),
                "bit_exact_samples": exact, "changed_samples": reference.pixels.len() - exact,
                "outside_test_rectangle_changed": outside_changed,
                "outside_test_rectangle_max_abs_difference": outside_max_difference,
                "max_abs_difference": max_difference,
                "rms_difference": (sum_square_difference / reference.pixels.len() as f64).sqrt(),
                "reference_range": original_range, "saved_range": range,
                "first_outside_differences": examples, "profile": profile(&saved)?
            })
        );
    }
    Ok(())
}
