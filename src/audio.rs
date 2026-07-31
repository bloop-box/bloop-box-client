use crate::hardware::asset::AssetLoader;
use crate::hardware::buttons::{Button, ButtonReceiver};
use crate::state::PersistedState;
use anyhow::{bail, Error, Result};
use bloop_client_framework::audio::{self, PlaybackHandle};
use rand_distr::weighted::WeightedAliasIndex;
use rand_distr::Distribution;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::sync::{mpsc, Mutex};
use tokio_graceful_shutdown::{FutureExt, IntoSubsystem, SubsystemHandle};
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct AudioPlayer {
    volume: Arc<Mutex<f32>>,
    asset_loader: AssetLoader,
    bloop_collection: Arc<AudioCollection>,
    award_collection: Arc<AudioCollection>,
}

impl AudioPlayer {
    pub async fn new() -> Result<Self> {
        let asset_loader = AssetLoader::new();
        let bloop_collection = AudioCollection::from_dir(&asset_loader, "bloops").await?;
        let award_collection = AudioCollection::from_dir(&asset_loader, "awards").await?;

        Ok(Self {
            volume: Arc::new(Mutex::new(1.0)),
            asset_loader,
            bloop_collection: Arc::new(bloop_collection),
            award_collection: Arc::new(award_collection),
        })
    }

    pub async fn play_bloop(&mut self) -> Result<()> {
        let path = self.bloop_collection.choose_random().clone();
        self.play_file(path).await
    }

    pub async fn play_award(&mut self) -> Result<()> {
        let path = self.award_collection.choose_random().clone();
        self.play_file(path).await
    }

    pub async fn play_error(&mut self) -> Result<()> {
        self.play_asset("error.mp3").await
    }

    pub async fn play_throttled(&mut self) -> Result<()> {
        self.play_asset("throttle.mp3").await
    }

    pub async fn play_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let volume = *self.volume.lock().await;
        play_logged(audio::play_file(path.as_ref(), volume)).await;
        Ok(())
    }

    pub async fn play_asset<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let volume = *self.volume.lock().await;

        match self.read_asset(path.as_ref()).await {
            Ok(reader) => play_logged(audio::play_reader(reader, volume)).await,
            Err(error) => error!("failed to play audio: {}", error),
        }

        Ok(())
    }

    async fn set_volume(&self, volume: f32, silent: bool) {
        let volume = volume.clamp(0.0, 1.0);
        *self.volume.lock().await = volume;

        if !silent {
            match self.read_asset(Path::new("volume-change.mp3")).await {
                Ok(reader) => audio::play_reader(reader, volume).detach(),
                Err(error) => error!("failed to play audio: {}", error),
            }
        }
    }

    /// Opens the asset off the runtime thread; on the Pi this is a
    /// synchronous SD card access.
    async fn read_asset(
        &self,
        path: &Path,
    ) -> Result<impl std::io::Read + std::io::Seek + Send + Sync + 'static> {
        let asset_loader = self.asset_loader.clone();
        let path = path.to_path_buf();

        tokio::task::spawn_blocking(move || asset_loader.read_file(path)).await?
    }
}

async fn play_logged(handle: PlaybackHandle) {
    if let Err(error) = handle.await {
        error!("failed to play audio: {}", error);
    }
}

#[derive(Debug)]
struct AudioCollection {
    paths: Vec<PathBuf>,
    dist: WeightedAliasIndex<f64>,
}

impl AudioCollection {
    pub async fn from_dir<P: AsRef<Path>>(
        asset_loader: &AssetLoader,
        path: P,
    ) -> Result<AudioCollection> {
        let paths = asset_loader
            .list_files(&path)
            .await?
            .into_iter()
            .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("mp3"))
            .collect::<Vec<_>>();

        if paths.is_empty() {
            bail!("path {} contains no mp3 files", path.as_ref().display());
        }

        let mut weights: Vec<f64> = Vec::new();
        let weight_regex = Regex::new(r"\.\[w=(\d+(?:\.\d*)?)]\.mp3$")?;

        for path in &paths {
            let Some(filename) = path.file_name() else {
                continue;
            };
            let capture = weight_regex.captures(filename.to_str().unwrap());

            weights.push(if let Some(cap) = capture {
                cap[1].parse::<f64>()?
            } else {
                100.
            });
        }

        Ok(AudioCollection {
            paths,
            dist: WeightedAliasIndex::new(weights)?,
        })
    }

    pub fn choose_random(&self) -> &PathBuf {
        &self.paths[self.dist.sample(&mut rand::rng())]
    }
}

pub struct VolumeControlTask {
    range_update_rx: mpsc::Receiver<(f32, f32)>,
    button_rx: ButtonReceiver,
    audio_player: AudioPlayer,
    state: PersistedState<VolumeState>,
}

impl VolumeControlTask {
    pub async fn new(
        range_rx: mpsc::Receiver<(f32, f32)>,
        button_rx: ButtonReceiver,
        audio_player: AudioPlayer,
    ) -> Result<Self> {
        let state =
            PersistedState::<VolumeState>::new("volume", Some(Duration::from_secs(5))).await?;
        audio_player.set_volume(state.current, true).await;

        Ok(Self {
            range_update_rx: range_rx,
            button_rx,
            audio_player,
            state,
        })
    }

    pub async fn listen(&mut self) -> Result<()> {
        loop {
            select! {
                Some(button) = self.button_rx.recv() => {
                    self.handle_button_press(&button).await?;
                },
                Some(range) = self.range_update_rx.recv() => {
                    self.handle_range_update(range).await?;
                },
                else => break,
            }
        }

        Ok(())
    }

    async fn handle_button_press(&mut self, button: &Button) -> Result<()> {
        let delta = match button {
            Button::VolumeUp => 0.05,
            Button::VolumeDown => -0.05,
        };

        let volume = (self.state.current + delta).clamp(self.state.min, self.state.max);
        self.state.mutate(|state| state.current = volume)?;
        self.audio_player.set_volume(volume, false).await;

        info!("volume set to {}", volume);
        Ok(())
    }

    async fn handle_range_update(&mut self, range: (f32, f32)) -> Result<()> {
        let min = range.0.clamp(0.0, 1.0);
        let max = range.1.clamp(min, 1.0);
        let current = self.state.current.clamp(min, max);

        self.state.mutate(|state| {
            state.min = range.0;
            state.max = range.1;
            state.current = current;
        })?;
        self.audio_player.set_volume(current, false).await;

        info!("volume range set to {} - {}", min, max);
        Ok(())
    }
}

impl IntoSubsystem<Error> for VolumeControlTask {
    async fn run(mut self, subsys: &mut SubsystemHandle) -> Result<()> {
        if let Ok(result) = self.listen().cancel_on_shutdown(subsys).await {
            result?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
struct VolumeState {
    current: f32,
    min: f32,
    max: f32,
}

impl Default for VolumeState {
    fn default() -> Self {
        Self {
            current: 1.0,
            min: 0.0,
            max: 1.0,
        }
    }
}

impl<'de> Deserialize<'de> for VolumeState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawVolumeState {
            current: f32,
            min: f32,
            max: f32,
        }

        let raw = RawVolumeState::deserialize(deserializer)?;
        let min = raw.min.clamp(0.0, 1.0);
        let max = raw.max.clamp(min, 1.0);

        Ok(Self {
            current: raw.current.clamp(0.0, 1.0),
            min,
            max,
        })
    }
}
