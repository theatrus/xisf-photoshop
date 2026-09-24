//! Persistent, content-addressed storage for binary metadata too large for XMP.
//! References contain a digest and size, never an arbitrary filesystem path.
use crate::Result;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::PathBuf,
};

pub(crate) const INLINE_BUDGET: usize = 8 * 1024 * 1024;
const PREFIX: &str = "cache:";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Default)]
pub(crate) struct Store {
    // Tests use an isolated directory without changing process environment.
    pub(crate) root: Option<PathBuf>,
}

pub(crate) enum Block {
    Inline(Vec<u8>),
    Stored(File, usize),
}

impl Block {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Inline(bytes) => bytes.len(),
            Self::Stored(_, size) => *size,
        }
    }

    pub(crate) fn write(self, writer: &mut impl Write) -> Result<()> {
        match self {
            Self::Inline(bytes) => writer.write_all(&bytes).map_err(|e| e.to_string()),
            Self::Stored(file, size) => {
                let copied = std::io::copy(&mut file.take(size as u64), writer)
                    .map_err(|e| e.to_string())?;
                if copied != size as u64 {
                    return Err("Retained metadata changed during save".into());
                }
                Ok(())
            }
        }
    }
}

impl Store {
    fn directory(&self) -> Result<PathBuf> {
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        #[cfg(target_os = "windows")]
        let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let root =
            std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let root = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")));
        Ok(root
            .ok_or("Cannot locate the local astronomy metadata folder")?
            .join("Seiza/Photoshop/Metadata"))
    }

    pub(crate) fn retain(&self, data: &[u8]) -> Result<String> {
        let digest = hex(&Sha256::digest(data));
        let reference = format!("{PREFIX}{digest}:{}", data.len());
        let root = self.directory()?;
        fs::create_dir_all(&root)
            .map_err(|e| format!("Cannot create metadata folder {}: {e}", root.display()))?;
        let path = root.join(format!("{digest}.bin"));
        if path.exists() && self.open(&reference, usize::MAX).is_ok() {
            return Ok(reference);
        }
        let mut temp = tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
        temp.write_all(data)
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|e| format!("Cannot retain binary metadata: {e}"))?;
        // Atomic replacement also repairs a damaged copy when reopening a source.
        temp.persist(&path)
            .map_err(|e| format!("Cannot retain binary metadata at {}: {e}", path.display()))?;
        Ok(reference)
    }

    pub(crate) fn open(&self, encoded: &str, limit: usize) -> Result<Block> {
        let Some(reference) = encoded.strip_prefix(PREFIX) else {
            if encoded.len() > limit.saturating_add(2).saturating_div(3).saturating_mul(4) {
                return Err("Encoded metadata block exceeds the size limit".into());
            }
            let bytes = B64.decode(encoded).map_err(|e| e.to_string())?;
            if bytes.len() > limit {
                return Err("Metadata block exceeds the size limit".into());
            }
            return Ok(Block::Inline(bytes));
        };
        let (digest, size) = reference
            .split_once(':')
            .ok_or("Invalid metadata reference")?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("Invalid metadata digest".into());
        }
        let size = size.parse::<usize>().map_err(|_| "Invalid metadata size")?;
        if size > limit {
            return Err("Metadata block exceeds the size limit".into());
        }
        let path = self.directory()?.join(format!("{digest}.bin"));
        let recovery = || {
            format!(
                "Reopen the original XISF or restore the metadata folder {}",
                path.parent().unwrap().display()
            )
        };
        let mut file = File::open(&path).map_err(|e| {
            format!(
                "Retained binary metadata is unavailable: {e}. {}",
                recovery()
            )
        })?;
        if file.metadata().map_err(|e| e.to_string())?.len() != size as u64 {
            return Err(format!(
                "Retained binary metadata has the wrong size. {}",
                recovery()
            ));
        }
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut remaining = size;
        while remaining != 0 {
            let count = remaining.min(buffer.len());
            file.read_exact(&mut buffer[..count])
                .map_err(|e| e.to_string())?;
            hash.update(&buffer[..count]);
            remaining -= count;
        }
        if hex(&hash.finalize()) != digest {
            return Err(format!(
                "Retained binary metadata failed its integrity check. {}",
                recovery()
            ));
        }
        file.rewind().map_err(|e| e.to_string())?;
        Ok(Block::Stored(file, size))
    }
}

pub(crate) fn bytes(encoded: &str, limit: usize) -> Result<Vec<u8>> {
    let block = Store::default().open(encoded, limit)?;
    let mut bytes = Vec::with_capacity(block.len());
    block.write(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_size_digest_and_repairs_storage_from_source() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store {
            root: Some(directory.path().to_owned()),
        };
        let reference = store.retain(b"original data").unwrap();
        assert_eq!(store.retain(b"original data").unwrap(), reference);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(store.open(&reference, 1).is_err());
        let path = fs::read_dir(directory.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(&path, b"changed! data").unwrap();
        assert!(store.open(&reference, usize::MAX).is_err());
        assert_eq!(store.retain(b"original data").unwrap(), reference);
        let mut bytes = Vec::new();
        store
            .open(&reference, usize::MAX)
            .unwrap()
            .write(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"original data");
        fs::write(&path, b"short").unwrap();
        assert!(store.open(&reference, usize::MAX).is_err());
        fs::remove_file(&path).unwrap();
        assert!(store.open(&reference, usize::MAX).is_err());
    }

    #[test]
    fn rejects_paths_malformed_references_and_oversized_inline_data() {
        let store = Store::default();
        for reference in [
            "cache:../../file:12",
            "cache:C:\\file:12",
            "cache:bad",
            "not base64",
        ] {
            assert!(store.open(reference, usize::MAX).is_err());
        }
        let digest = "a".repeat(64);
        for size in ["-1", "18446744073709551616", "1:2"] {
            assert!(
                store
                    .open(&format!("cache:{digest}:{size}"), usize::MAX)
                    .is_err()
            );
        }
        assert!(store.open(&B64.encode([1, 2, 3, 4]), 3).is_err());
    }
}
