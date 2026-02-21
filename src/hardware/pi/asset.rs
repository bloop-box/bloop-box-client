use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone)]
pub struct AssetLoader {
    base_path: PathBuf,
}

impl AssetLoader {
    pub fn new() -> Self {
        Self {
            base_path: PathBuf::from("/usr/share/bloop-box"),
        }
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> Result<BufReader<File>> {
        let file = File::open(self.base_path.join(&path))
            .with_context(|| anyhow!("failed to open file: {}", path.as_ref().display()))?;

        Ok(BufReader::new(file))
    }

    pub async fn list_files<P: AsRef<Path>>(&self, path: P) -> Result<Vec<PathBuf>> {
        let mut entries = fs::read_dir(self.base_path.join(&path))
            .await
            .with_context(|| anyhow!("failed to read directory: {}", path.as_ref().display()))?;

        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            files.push(entry.path());
        }

        Ok(files)
    }
}
