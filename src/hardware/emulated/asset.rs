use anyhow::{Context, Result};
use include_dir::{include_dir, Dir};
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

static SHARE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/emulation-assets");

#[derive(Debug, Clone)]
pub struct AssetLoader;

impl AssetLoader {
    pub fn new() -> Self {
        Self {}
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> Result<BufReader<Cursor<Vec<u8>>>> {
        let bytes = SHARE_DIR
            .get_file(&path)
            .with_context(|| format!("File {} not found", path.as_ref().display()))?
            .contents()
            .to_vec();

        Ok(BufReader::new(Cursor::new(bytes)))
    }

    pub async fn list_files<P: AsRef<Path>>(&self, path: P) -> Result<Vec<PathBuf>> {
        let entries = SHARE_DIR
            .get_dir(&path)
            .with_context(|| format!("directory {} not found", path.as_ref().display()))?
            .entries();

        Ok(entries
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect())
    }
}
