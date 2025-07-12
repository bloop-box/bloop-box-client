use crate::hardware::data_path;
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;
use tokio::time::sleep;
use tokio::{fs, io, task};
use tracing::{info, instrument, warn};

#[derive(Debug)]
pub struct PersistedState<T> {
    state: T,
    tx: watch::Sender<T>,
}

impl<T> PersistedState<T>
where
    T: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static,
{
    pub async fn new(name: impl Into<String>, debounce: Option<Duration>) -> Result<Self> {
        let data_path = data_path().await?;
        let filename = format!("{}.state", name.into());
        let full_path = data_path.join(filename);
        let state = Self::load_state(&full_path).await;
        let (tx, rx) = watch::channel(state.clone());

        task::spawn(async move {
            persistence_task(full_path, debounce, rx).await;
        });

        Ok(Self { state, tx })
    }

    pub fn mutate<F: FnOnce(&mut T)>(&mut self, f: F) -> Result<()> {
        f(&mut self.state);
        self.tx
            .send(self.state.clone())
            .context("failed to send state update to persistence task")?;

        Ok(())
    }

    #[instrument]
    async fn load_state(full_path: &Path) -> T {
        info!("loading state");

        let mut file = match File::open(&full_path).await {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                info!("state file not found, using default");
                return T::default();
            }
            Err(err) => {
                warn!(
                    "failed to open state file, falling back to default: {}",
                    err
                );
                return T::default();
            }
        };

        let mut raw_toml = String::new();

        if let Err(err) = file.read_to_string(&mut raw_toml).await {
            warn!(
                "failed to read state file, falling back to default: {}",
                err
            );
            return T::default();
        }

        toml::from_str::<T>(&raw_toml).unwrap_or_else(|err| {
            warn!(
                "failed to parse state file, falling back to default: {}",
                err
            );
            T::default()
        })
    }
}

impl<T> Deref for PersistedState<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

#[instrument(skip(rx))]
async fn persistence_task<T>(path: PathBuf, debounce: Option<Duration>, mut rx: watch::Receiver<T>)
where
    T: Serialize,
{
    let tmp_file_path = path.with_extension("tmp");

    loop {
        if rx.changed().await.is_err() {
            break;
        };

        if let Some(debounce) = debounce {
            sleep(debounce).await;
            let Ok(has_changed) = rx.has_changed() else {
                break;
            };

            if has_changed {
                continue;
            }
        }

        let raw_toml = match toml::to_string_pretty(&*rx.borrow()) {
            Ok(raw_toml) => raw_toml,
            Err(err) => {
                warn!("failed to serialized state: {}", err);
                continue;
            }
        };
        let mut file = match File::create(&tmp_file_path).await {
            Ok(file) => file,
            Err(err) => {
                warn!(
                    "failed to create temporary state file {}: {}",
                    tmp_file_path.display(),
                    err
                );
                continue;
            }
        };

        if let Err(err) = file.write_all(raw_toml.as_bytes()).await {
            warn!(
                "failed to write temporary state file {}: {}",
                tmp_file_path.display(),
                err
            );
            continue;
        }

        if let Err(err) = fs::rename(&tmp_file_path, &path).await {
            warn!(
                "failed to rename temporary state file {}: {}",
                tmp_file_path.display(),
                err
            );
        }

        info!("state persisted");
    }
}
