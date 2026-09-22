//! Astronomy metadata travels with the Photoshop document in a private XMP
//! namespace. Native headers remain readable by other astronomy applications.
//! Format switches copy compatible FITS keywords; same-format saves retain the
//! full source metadata without imposing a reduced key/value schema.
use crate::{Format, Result};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use std::io::Write;

pub const LIMIT: usize = 64 * 1024 * 1024;
const NS: &str = "https://seiza.fyi/ns/photoshop/1.0/";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    version: u32,
    width: usize,
    height: usize,
    pub debayered: bool,
    pub cards: Vec<String>,
    xml: Option<String>,
    blocks: Vec<String>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            version: 1,
            width: 0,
            height: 0,
            debayered: false,
            cards: Vec::new(),
            xml: None,
            blocks: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
enum Node {
    Element {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
    Text(String),
    Comment(String),
}
impl Node {
    fn element(name: &str) -> Self {
        Self::Element {
            name: name.into(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }
    fn name(&self) -> &str {
        match self {
            Self::Element { name, .. } => name,
            _ => "",
        }
    }
    fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Self::Element { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
    fn set(&mut self, key: &str, value: impl Into<String>) {
        if let Self::Element { attrs, .. } = self {
            attrs.retain(|(k, _)| k != key);
            attrs.push((key.into(), value.into()));
        }
    }
    fn remove(&mut self, key: &str) {
        if let Self::Element { attrs, .. } = self {
            attrs.retain(|(k, _)| k != key);
        }
    }
    fn children(&mut self) -> &mut Vec<Node> {
        match self {
            Self::Element { children, .. } => children,
            _ => unreachable!(),
        }
    }
    fn render(&self, out: &mut String) {
        match self {
            Self::Element {
                name,
                attrs,
                children,
            } => {
                out.push('<');
                out.push_str(name);
                for (key, value) in attrs {
                    out.push(' ');
                    out.push_str(key);
                    out.push_str("=\"");
                    out.push_str(&quick_xml::escape::escape(value));
                    out.push('"');
                }
                if children.is_empty() {
                    out.push_str("/>");
                } else {
                    out.push('>');
                    for child in children {
                        child.render(out);
                    }
                    out.push_str("</");
                    out.push_str(name);
                    out.push('>');
                }
            }
            Self::Text(s) => out.push_str(&quick_xml::escape::escape(s)),
            Self::Comment(s) => {
                out.push_str("<!--");
                out.push_str(s);
                out.push_str("-->");
            }
        }
    }
    fn xml(&self) -> String {
        let mut out = String::new();
        self.render(&mut out);
        out
    }
}

fn parse(xml: &str) -> Result<Node> {
    if xml.len() > LIMIT {
        return Err("Metadata XML exceeds 64 MiB".into());
    }
    let mut reader = Reader::from_str(xml);
    let mut stack = vec![Node::element("document")];
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                if stack.len() > 64 {
                    return Err("Metadata XML is too deeply nested".into());
                }
                let mut node = Node::element(
                    std::str::from_utf8(e.name().as_ref()).map_err(|e| e.to_string())?,
                );
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| e.to_string())?;
                    node.set(
                        std::str::from_utf8(attr.key.as_ref()).map_err(|e| e.to_string())?,
                        attr.decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(|e| e.to_string())?
                        .into_owned(),
                    );
                }
                if matches!(event, Event::Empty(_)) {
                    stack.last_mut().unwrap().children().push(node);
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                if stack.len() < 2 {
                    return Err("Unbalanced metadata XML".into());
                }
                let node = stack.pop().unwrap();
                stack.last_mut().unwrap().children().push(node);
            }
            Event::Text(e) => stack.last_mut().unwrap().children().push(Node::Text(
                e.decode().map_err(|e| e.to_string())?.into_owned(),
            )),
            Event::CData(e) => stack.last_mut().unwrap().children().push(Node::Text(
                e.decode().map_err(|e| e.to_string())?.into_owned(),
            )),
            Event::GeneralRef(e) => {
                let reference = format!("&{};", e.decode().map_err(|e| e.to_string())?);
                stack.last_mut().unwrap().children().push(Node::Text(
                    quick_xml::escape::unescape(&reference)
                        .map_err(|e| e.to_string())?
                        .into_owned(),
                ));
            }
            Event::Comment(e) => stack.last_mut().unwrap().children().push(Node::Comment(
                e.decode().map_err(|e| e.to_string())?.into_owned(),
            )),
            Event::DocType(_) => {
                return Err("DTD declarations are not supported in metadata".into());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err("Incomplete metadata XML".into());
    }
    let roots: Vec<_> = stack
        .pop()
        .unwrap()
        .children()
        .drain(..)
        .filter(|n| !n.name().is_empty())
        .collect();
    if roots.len() != 1 {
        return Err("Metadata XML must have one root".into());
    }
    Ok(roots.into_iter().next().unwrap())
}

fn image(root: &mut Node) -> Result<&mut Node> {
    root.children()
        .iter_mut()
        .find(|n| n.name() == "Image")
        .ok_or_else(|| "Missing XISF image metadata".into())
}
fn keyword(card: &str) -> &str {
    card.get(..8).unwrap_or(card).trim()
}
fn structural(key: &str) -> bool {
    matches!(
        key,
        "SIMPLE"
            | "XTENSION"
            | "BITPIX"
            | "NAXIS"
            | "EXTEND"
            | "PCOUNT"
            | "GCOUNT"
            | "BSCALE"
            | "BZERO"
            | "BLANK"
            | "DATAMIN"
            | "DATAMAX"
            | "CHECKSUM"
            | "DATASUM"
            | "END"
    ) || key
        .strip_prefix("NAXIS")
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
fn cfa(key: &str) -> bool {
    matches!(
        key,
        "BAYERPAT" | "XBAYROFF" | "YBAYROFF" | "BAYEROFF" | "BAYERX" | "BAYERY"
    )
}
fn wcs(key: &str) -> bool {
    matches!(
        key,
        "WCSAXES" | "WCSNAME" | "RADESYS" | "RADECSYS" | "EQUINOX" | "LONPOLE" | "LATPOLE"
    ) || [
        "CTYPE", "CRPIX", "CRVAL", "CDELT", "CUNIT", "CROTA", "CD", "PC", "PV", "PS", "A_", "B_",
        "AP_", "BP_",
    ]
    .iter()
    .any(|p| {
        key.strip_prefix(p)
            .is_some_and(|s| p.ends_with('_') || s.starts_with(|c: char| c.is_ascii_digit()))
    })
}
fn header(bytes: &[u8]) -> Result<(Vec<String>, usize)> {
    let mut cards = Vec::new();
    for (i, card) in bytes
        .as_chunks::<80>()
        .0
        .iter()
        .take(LIMIT / 80)
        .enumerate()
    {
        let card = std::str::from_utf8(card).map_err(|_| "Non-ASCII FITS header")?;
        if !card.is_ascii() {
            return Err("Non-ASCII FITS header".into());
        }
        if keyword(card) == "END" {
            return Ok((cards, ((i + 1) * 80).div_ceil(2880) * 2880));
        }
        cards.push(card.to_owned());
    }
    Err("Missing FITS metadata END card".into())
}
fn fits_value(card: &str) -> (&str, &str) {
    let text = card.get(10..).unwrap_or("");
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        }
        if c == '/' && !quoted {
            return (text[..i].trim(), text[i + 1..].trim());
        }
    }
    (text.trim(), "")
}
fn write_header(cards: &[String], writer: &mut impl Write) -> Result<()> {
    let mut bytes = Vec::new();
    for card in cards
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("END"))
    {
        if !card.is_ascii() || card.len() > 80 {
            return Err("Invalid preserved FITS card".into());
        }
        bytes.extend_from_slice(card.as_bytes());
        bytes.resize(bytes.len().div_ceil(80) * 80, b' ');
    }
    bytes.resize(bytes.len().div_ceil(2880) * 2880, b' ');
    writer.write_all(&bytes).map_err(|e| e.to_string())
}

impl Metadata {
    fn json(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > LIMIT {
            return Err("Astronomy metadata exceeds 64 MiB".into());
        }
        Ok(bytes)
    }
    fn from_json(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > LIMIT {
            return Err("Astronomy metadata exceeds 64 MiB".into());
        }
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("Invalid astronomy metadata: {e}"))?;
        if value.version != 1 {
            return Err("Unsupported astronomy metadata version".into());
        }
        if value.cards.iter().any(|c| c.len() != 80 || !c.is_ascii()) {
            return Err("Invalid preserved FITS cards".into());
        }
        Ok(value)
    }
    pub fn xmp(&self) -> Result<Vec<u8>> {
        let payload = B64.encode(self.json()?);
        Ok(format!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:seiza=\"{NS}\"><seiza:Metadata>{payload}</seiza:Metadata></rdf:Description></rdf:RDF></x:xmpmeta>").into_bytes())
    }
    pub fn from_xmp(bytes: &[u8]) -> Result<Option<Self>> {
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.len() > LIMIT * 2 {
            return Err("Document XMP exceeds metadata size limit".into());
        }
        let xml = std::str::from_utf8(bytes)
            .map_err(|e| e.to_string())?
            .trim_end_matches('\0');
        let doc =
            roxmltree::Document::parse(xml).map_err(|e| format!("Invalid document XMP: {e}"))?;
        let mut values = Vec::new();
        for node in doc.descendants().filter(|n| n.is_element()) {
            if node.tag_name().namespace() == Some(NS) && node.tag_name().name() == "Metadata" {
                values.push(node.text().unwrap_or(""));
            }
            // Photoshop can canonicalize a simple XMP element to an attribute.
            for attr in node.attributes() {
                if attr.namespace() == Some(NS) && attr.name() == "Metadata" {
                    values.push(attr.value());
                }
            }
        }
        if values.len() > 1 {
            return Err("Ambiguous astronomy metadata in document XMP".into());
        }
        values
            .first()
            .map(|s| {
                B64.decode(s.trim())
                    .map_err(|e| e.to_string())
                    .and_then(|v| Self::from_json(&v))
            })
            .transpose()
    }

    pub fn read(format: Format, bytes: &[u8], width: usize, height: usize) -> Result<Self> {
        let mut result = Self {
            width,
            height,
            ..Self::default()
        };
        match format {
            Format::Fits => {
                let (cards, _) = header(bytes)?;
                result.cards = cards
                    .into_iter()
                    .filter(|c| !structural(keyword(c)))
                    .collect();
            }
            Format::Xisf => {
                let length = u32::from_le_bytes(
                    bytes
                        .get(8..12)
                        .ok_or("Truncated XISF header")?
                        .try_into()
                        .unwrap(),
                ) as usize;
                let xml = std::str::from_utf8(
                    bytes
                        .get(16..16 + length)
                        .ok_or("Truncated XISF metadata")?,
                )
                .map_err(|e| e.to_string())?;
                let mut root = parse(xml.trim_end_matches('\0'))?;
                if root.name() != "xisf" {
                    return Err("Invalid XISF metadata root".into());
                }
                let mut first = true;
                root.children().retain(|n| {
                    if n.name() == "Image" {
                        let keep = first;
                        first = false;
                        keep
                    } else {
                        true
                    }
                });
                let img = image(&mut root)?;
                // Pixel storage is regenerated. Never archive a second copy of the image.
                for key in [
                    "location",
                    "compression",
                    "subblocks",
                    "checksum",
                    "bounds",
                    "byteOrder",
                    "pixelStorage",
                    "sampleFormat",
                    "geometry",
                    "colorSpace",
                ] {
                    img.remove(key);
                }
                {
                    for node in img.children().iter().filter(|n| n.name() == "FITSKeyword") {
                        let key = node.attr("name").unwrap_or("");
                        if key.is_empty() || key.len() > 8 || !key.is_ascii() || structural(key) {
                            continue;
                        }
                        let value = node.attr("value").unwrap_or("");
                        let comment = node.attr("comment").unwrap_or("");
                        let mut card = if matches!(key, "COMMENT" | "HISTORY") {
                            format!("{key:8}{value}{comment}")
                        } else {
                            format!("{key:8}= {value}")
                        };
                        if !comment.is_empty() && !matches!(key, "COMMENT" | "HISTORY") {
                            card.push_str(" / ");
                            card.push_str(comment);
                        }
                        // Nonrepresentable values stay in native XISF metadata only.
                        if card.len() <= 80 && card.is_ascii() {
                            result.cards.push(format!("{card:80}"));
                        }
                    }
                }
                img.children().retain(|n| n.name() != "Thumbnail");
                collect_blocks(&mut root, bytes, &mut result.blocks)?;
                result.xml = Some(root.xml());
            }
        }
        result.json()?; // Apply the bound before handing metadata to the host.
        Ok(result)
    }

    fn adjusted(&self, width: usize, height: usize, planes: usize) -> Result<Self> {
        let mut result = self.clone();
        let resized = self.width != 0 && (width != self.width || height != self.height);
        let remove_cfa = self.debayered || planes != 1 || resized;
        result.cards.retain(|c| {
            let k = keyword(c);
            !structural(k) && !(remove_cfa && cfa(k)) && !(resized && wcs(k))
        });
        if let Some(xml) = &self.xml {
            let mut root = parse(xml)?;
            clean(&mut root, remove_cfa, resized);
            result.xml = Some(root.xml());
        }
        result.width = width;
        result.height = height;
        Ok(result)
    }
}

fn clean(node: &mut Node, remove_cfa: bool, resized: bool) {
    if node.name() == "Property" && node.attr("id") == Some("XISF:BlockAlignmentSize") {
        node.set("value", "4096");
    }
    if let Node::Element { children, .. } = node {
        children.retain(|n| {
            if n.name() == "Thumbnail" || (remove_cfa && n.name() == "ColorFilterArray") {
                return false;
            }
            if n.name() == "FITSKeyword" {
                let k = n.attr("name").unwrap_or("");
                return !structural(k) && !(remove_cfa && cfa(k)) && !(resized && wcs(k));
            }
            !(resized
                && n.name() == "Property"
                && n.attr("id")
                    .is_some_and(|id| id.starts_with("PCL:AstrometricSolution:")))
        });
        for child in children {
            clean(child, remove_cfa, resized);
        }
    }
}

fn collect_blocks(node: &mut Node, bytes: &[u8], blocks: &mut Vec<String>) -> Result<()> {
    if let Some(location) = node.attr("location").map(str::to_owned) {
        if let Some(spec) = location.strip_prefix("attachment:") {
            let (start, size) = spec.split_once(':').ok_or("Invalid metadata attachment")?;
            let start = start.parse::<usize>().map_err(|e| e.to_string())?;
            let size = size.parse::<usize>().map_err(|e| e.to_string())?;
            if size > LIMIT || blocks.iter().map(String::len).sum::<usize>() + size > LIMIT {
                return Err("Metadata attachments exceed 64 MiB".into());
            }
            let data = bytes
                .get(
                    start
                        ..start
                            .checked_add(size)
                            .ok_or("Metadata attachment overflow")?,
                )
                .ok_or("Truncated metadata attachment")?;
            node.set("location", format!("seiza-block:{}", blocks.len()));
            blocks.push(B64.encode(data));
        } else if !location.starts_with("inline:") && location != "embedded" {
            return Err(
                "External XISF metadata blocks cannot be retained; use a monolithic XISF file"
                    .into(),
            );
        }
    }
    if let Node::Element { children, .. } = node {
        for child in children {
            collect_blocks(child, bytes, blocks)?;
        }
    }
    Ok(())
}

fn locate_blocks(node: &mut Node, offsets: &[(usize, usize)]) -> Result<()> {
    if let Some(value) = node
        .attr("location")
        .and_then(|s| s.strip_prefix("seiza-block:"))
    {
        let index = value.parse::<usize>().map_err(|e| e.to_string())?;
        let (start, length) = offsets
            .get(index)
            .ok_or("Invalid metadata block reference")?;
        node.set("location", format!("attachment:{start}:{length}"));
    }
    if let Node::Element { children, .. } = node {
        for child in children {
            locate_blocks(child, offsets)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn encode(
    format: Format,
    depth: u32,
    width: usize,
    height: usize,
    planes: usize,
    pixels: &[f32],
    metadata: &Metadata,
    mut writer: impl Write,
) -> Result<()> {
    let metadata = metadata.adjusted(width, height, planes)?;
    let mut base = Vec::new();
    match depth {
        16 => crate::encode_u16_pixels(format, width, height, planes, pixels, &mut base)?,
        32 => crate::encode_pixels(format, width, height, planes, pixels, &mut base)?,
        _ => return Err("Output depth must be 16 or 32".into()),
    }
    let write = |w: &mut dyn Write, b: &[u8]| w.write_all(b).map_err(|e| e.to_string());
    match format {
        Format::Fits => {
            let (mut cards, start) = header(&base)?;
            cards.extend(metadata.cards.iter().cloned());
            write_header(&cards, &mut writer)?;
            write(&mut writer, &base[start..])?;
        }
        Format::Xisf => {
            let size = u32::from_le_bytes(base[8..12].try_into().unwrap()) as usize;
            let mut base_root =
                parse(std::str::from_utf8(&base[16..16 + size]).map_err(|e| e.to_string())?)?;
            let base_img = image(&mut base_root)?;
            let location = base_img
                .attr("location")
                .unwrap()
                .split(':')
                .collect::<Vec<_>>();
            let start = location[1].parse::<usize>().map_err(|e| e.to_string())?;
            let length = location[2].parse::<usize>().map_err(|e| e.to_string())?;
            let pixels = base
                .get(start..start + length)
                .ok_or("Invalid encoded XISF pixels")?;
            let mut root = if let Some(xml) = &metadata.xml {
                parse(xml)?
            } else {
                base_root.clone()
            };
            let img = image(&mut root)?;
            if let Node::Element { attrs, .. } = image(&mut base_root)? {
                for (k, v) in attrs {
                    img.set(k, v.clone());
                }
            }
            if metadata.xml.is_none() {
                for card in &metadata.cards {
                    let key = keyword(card);
                    let (value, comment) = if matches!(key, "COMMENT" | "HISTORY" | "") {
                        (card[8..].trim(), "")
                    } else {
                        fits_value(card)
                    };
                    let mut node = Node::element("FITSKeyword");
                    node.set("name", key);
                    node.set("value", value);
                    node.set("comment", comment);
                    img.children().push(node);
                }
            }
            let blocks: Vec<Vec<u8>> = metadata
                .blocks
                .iter()
                .map(|s| B64.decode(s).map_err(|e| e.to_string()))
                .collect::<Result<_>>()?;
            let mut data_start = 4096usize;
            let xml = loop {
                let mut positioned = root.clone();
                image(&mut positioned)?
                    .set("location", format!("attachment:{data_start}:{length}"));
                let mut end = data_start.checked_add(length).ok_or("XISF size overflow")?;
                let offsets: Vec<_> = blocks
                    .iter()
                    .map(|b| {
                        end = end.div_ceil(4096) * 4096;
                        let offset = (end, b.len());
                        end += b.len();
                        offset
                    })
                    .collect();
                locate_blocks(&mut positioned, &offsets)?;
                let xml = positioned.xml();
                if xml.len() > 16 * 1024 * 1024 {
                    return Err("XISF metadata header exceeds 16 MiB".into());
                }
                let required = (16 + xml.len()).div_ceil(4096) * 4096;
                if required <= data_start {
                    break xml;
                }
                data_start = required;
            };
            let mut header = b"XISF0100".to_vec();
            header.extend((xml.len() as u32).to_le_bytes());
            header.extend([0; 4]);
            header.extend(xml.as_bytes());
            header.resize(data_start, 0);
            write(&mut writer, &header)?;
            write(&mut writer, pixels)?;
            let mut position = data_start + pixels.len();
            for block in blocks {
                let padding = (4096 - position % 4096) % 4096;
                write(&mut writer, &vec![0; padding])?;
                write(&mut writer, &block)?;
                position += padding + block.len();
            }
        }
    }
    Ok(())
}
