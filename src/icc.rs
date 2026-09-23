//! Decode a retained XISF ICC data block without changing image samples.
use crate::Result;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use sha2::Digest;
use std::io::Read;

pub const LIMIT: usize = 16 * 1024 * 1024;

/// Check the ICC envelope and tag bounds before handing untrusted bytes to the host.
/// The host's color engine remains responsible for interpreting profile contents.
pub fn validate(data: &[u8]) -> Result<()> {
    if data.len() < 132 || data.len() > LIMIT || &data[36..40] != b"acsp" {
        return Err("Invalid ICC profile header or size (limit: 16 MiB)".into());
    }
    let number = |offset| u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
    if number(0) != data.len() {
        return Err("ICC profile length does not match its header".into());
    }
    let tags = number(128);
    if tags > (data.len() - 132) / 12 {
        return Err("Truncated ICC profile tag table".into());
    }
    let table_end = 132 + tags * 12;
    for tag in 0..tags {
        let start = number(136 + tag * 12);
        let size = number(140 + tag * 12);
        if start < table_end || start > data.len() || size > data.len() - start {
            return Err("ICC profile tag lies outside the profile".into());
        }
    }
    Ok(())
}

pub fn matches_planes(data: &[u8], planes: usize) -> bool {
    matches!(
        (data.get(16..20), planes),
        (Some(b"GRAY"), 1) | (Some(b"RGB "), 3)
    )
}

fn number(value: &str) -> Result<usize> {
    let size = value
        .parse::<usize>()
        .map_err(|_| "Invalid ICC block size")?;
    if size == 0 || size > LIMIT {
        return Err("ICC data block exceeds 16 MiB or has zero size".into());
    }
    Ok(size)
}

fn hex(value: &str) -> Result<Vec<u8>> {
    if !value.is_ascii() || !value.len().is_multiple_of(2) {
        return Err("Invalid hexadecimal ICC data".into());
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                .map_err(|_| "Invalid hexadecimal ICC data".into())
        })
        .collect()
}

fn text_data(node: roxmltree::Node<'_, '_>, encoding: &str) -> Result<Vec<u8>> {
    let value: String = node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .flat_map(str::chars)
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if value.len() > LIMIT * 2 {
        return Err("Encoded ICC data exceeds the size limit".into());
    }
    match encoding {
        "base64" => B64
            .decode(value)
            .map_err(|e| format!("Invalid ICC base64: {e}")),
        "hex" => hex(&value),
        _ => Err("Unsupported ICC data encoding".into()),
    }
}

fn decompress(codec: &str, stored: &[u8], size: usize) -> Result<Vec<u8>> {
    let reader: Box<dyn Read + '_> = match codec {
        "zlib" => Box::new(flate2::read::ZlibDecoder::new(stored)),
        "zstd" => Box::new(zstd::stream::read::Decoder::new(stored).map_err(|e| e.to_string())?),
        "lz4" | "lz4hc" => {
            let bytes = lz4_flex::block::decompress(stored, size).map_err(|e| e.to_string())?;
            if bytes.len() != size {
                return Err("Incorrect decompressed ICC block size".into());
            }
            return Ok(bytes);
        }
        _ => return Err("Unsupported ICC compression codec".into()),
    };
    let mut bytes = Vec::new();
    reader
        .take(size as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() != size {
        return Err("Incorrect decompressed ICC block size".into());
    }
    Ok(bytes)
}

pub(crate) fn decode(xml: &str, blocks: &[String]) -> Result<Vec<u8>> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let node = doc.root_element();
    let location = node
        .attribute("location")
        .ok_or("Missing ICC data location")?;
    let data_node = if location == "embedded" {
        let nodes: Vec<_> = node.children().filter(|n| n.has_tag_name("Data")).collect();
        if nodes.len() != 1 {
            return Err("ICC profile requires one embedded Data element".into());
        }
        nodes[0]
    } else {
        node
    };
    let attribute = |key| data_node.attribute(key).or_else(|| node.attribute(key));
    let stored = if let Some(index) = location.strip_prefix("seiza-block:") {
        let index = index
            .parse::<usize>()
            .map_err(|_| "Invalid ICC attachment index")?;
        let encoded = blocks.get(index).ok_or("Missing ICC attachment")?;
        if encoded.len() > LIMIT.div_ceil(3) * 4 {
            return Err("ICC attachment exceeds 16 MiB".into());
        }
        B64.decode(encoded).map_err(|e| e.to_string())?
    } else if let Some(encoding) = location.strip_prefix("inline:") {
        text_data(node, encoding)?
    } else if location == "embedded" {
        text_data(
            data_node,
            data_node
                .attribute("encoding")
                .ok_or("Missing ICC Data encoding")?,
        )?
    } else {
        return Err("Unsupported ICC data location".into());
    };
    if stored.len() > LIMIT {
        return Err("ICC attachment exceeds 16 MiB".into());
    }
    if let Some(checksum) = attribute("checksum") {
        let (algorithm, expected) = checksum.split_once(':').ok_or("Invalid ICC checksum")?;
        let actual = match algorithm {
            "sha1" | "sha-1" => sha1::Sha1::digest(&stored).to_vec(),
            "sha256" | "sha-256" => sha2::Sha256::digest(&stored).to_vec(),
            "sha512" | "sha-512" => sha2::Sha512::digest(&stored).to_vec(),
            _ => return Err("Unsupported ICC checksum algorithm".into()),
        };
        if actual != hex(expected)? {
            return Err("ICC profile checksum mismatch".into());
        }
    }
    let Some(compression) = attribute("compression") else {
        if attribute("subblocks").is_some() {
            return Err("ICC subblocks require compression".into());
        }
        validate(&stored)?;
        return Ok(stored);
    };
    let parts: Vec<_> = compression.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err("Invalid ICC compression descriptor".into());
    }
    let (codec, shuffled) = parts[0]
        .strip_suffix("+sh")
        .map_or((parts[0], false), |c| (c, true));
    let size = number(parts[1])?;
    if parts.len() != if shuffled { 3 } else { 2 } {
        return Err("Invalid ICC shuffle descriptor".into());
    }
    let mut bytes = if let Some(subblocks) = attribute("subblocks") {
        let mut bytes = Vec::new();
        let mut offset = 0usize;
        for block in subblocks.split(':') {
            let (packed, unpacked) = block
                .split_once(',')
                .ok_or("Invalid ICC compression subblock")?;
            let (packed, unpacked) = (number(packed)?, number(unpacked)?);
            if packed > stored.len() - offset || unpacked > size - bytes.len() {
                return Err("ICC compression subblock exceeds declared size".into());
            }
            let input = &stored[offset..offset + packed];
            if packed == unpacked {
                bytes.extend_from_slice(input);
            } else {
                bytes.extend(decompress(codec, input, unpacked)?);
            }
            offset += packed;
        }
        if offset != stored.len() || bytes.len() != size {
            return Err("Incomplete ICC compression subblocks".into());
        }
        bytes
    } else {
        decompress(codec, &stored, size)?
    };
    if shuffled {
        let item = number(parts[2])?;
        if item > size || !size.is_multiple_of(item) {
            return Err("Invalid ICC byte shuffle size".into());
        }
        let count = size / item;
        let mut output = vec![0; size];
        for byte in 0..item {
            for i in 0..count {
                output[i * item + byte] = bytes[byte * count + i];
            }
        }
        bytes = output;
    }
    validate(&bytes)?;
    Ok(bytes)
}
