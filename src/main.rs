use crate::audio::{AudioPlayer, VolumeControlTask};
use crate::engine::{Engine, EngineProps};
use crate::hardware::{init_hardware, HardwareContext, InitSubsystems, Peripherals};
use crate::network::task::{NetworkStatus, NetworkTask, RootCertSource};
use crate::network::NetworkClient;
#[cfg(feature = "hardware-emulation")]
use crate::thread::supervised_thread;
use crate::thread::unwrap_threads;
use anyhow::{bail, Result};
use std::env;
use std::future::Future;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, watch};
use tokio::time::Duration;
use tokio_graceful_shutdown::{
    FutureExt, IntoSubsystem, SubsystemBuilder, SubsystemHandle, Toplevel,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

mod audio;
mod engine;
mod hardware;
mod network;
mod state;
mod thread;

fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("error,bloop_box=info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let shutdown_token = CancellationToken::new();
    let hardware = init_hardware(shutdown_token.clone())?;

    run(hardware, shutdown_token)?;

    Ok(())
}

fn run_async_runtime(
    peripherals: Peripherals,
    init_subsystems: InitSubsystems,
    shutdown_token: CancellationToken,
) -> Result<()> {
    RuntimeWithInstantShutdown::new().block_on(async {
        let root_cert_source = match env::var("BLOOP_BOX_ROOT_CERT_SOURCE")
            .unwrap_or_default()
            .as_str()
        {
            "built_in" | "" => RootCertSource::BuiltIn,
            "native" => RootCertSource::Native,
            "dangerous_disabled" => RootCertSource::DangerousDisabled,
            value => bail!("invalid value for BLOOP_BOX_ROOT_CERT_SOURCE: {value}"),
        };

        let start_subsystems = init_subsystems()?;

        let audio_player = AudioPlayer::new().await?;
        let (volume_range_tx, volume_range_rx) = mpsc::channel(16);
        let volume_control_task = VolumeControlTask::new(
            volume_range_rx,
            peripherals.button_receiver,
            audio_player.clone(),
        )
        .await?;

        let (network_command_tx, network_command_rx) = mpsc::channel(16);
        let (network_status_tx, network_status_rx) = watch::channel(NetworkStatus::Disconnected);
        let network_task =
            NetworkTask::new(root_cert_source, network_command_rx, network_status_tx).await?;
        let network_client = NetworkClient::new(network_command_tx);

        let engine = Engine::new(EngineProps {
            led_controller: peripherals.led_controller,
            nfc_reader: peripherals.nfc_reader,
            network_client,
            audio_player,
            network_status: network_status_rx,
            volume_range_tx,
        })
        .await?;

        let root_subsystem = async |s: SubsystemHandle| {
            start_subsystems(&s);

            s.start(SubsystemBuilder::new(
                "ShutdownDetector",
                async move |s| -> Result<()> {
                    if shutdown_token
                        .cancelled()
                        .cancel_on_shutdown(&s)
                        .await
                        .is_ok()
                    {
                        s.request_shutdown();
                    }

                    Ok(())
                },
            ));

            s.start(SubsystemBuilder::new(
                "VolumeControl",
                volume_control_task.into_subsystem(),
            ));
            s.start(SubsystemBuilder::new(
                "Network",
                network_task.into_subsystem(),
            ));
            s.start(SubsystemBuilder::new("Engine", engine.into_subsystem()));
        };

        Toplevel::new(root_subsystem)
            .catch_signals()
            .handle_shutdown_requests(Duration::from_millis(1000))
            .await
            .map_err(Into::into)
    })
}

#[cfg(not(feature = "hardware-emulation"))]
fn run(hardware_context: HardwareContext, shutdown_token: CancellationToken) -> Result<()> {
    let result = run_async_runtime(
        hardware_context.peripherals,
        hardware_context.init_subsystems,
        shutdown_token.clone(),
    );
    shutdown_token.cancel();
    let threads_had_errors = unwrap_threads(hardware_context.threads);

    result?;

    if threads_had_errors {
        bail!("At least one thread failed");
    }

    Ok(())
}

#[cfg(feature = "hardware-emulation")]
fn run(hardware_context: HardwareContext, shutdown_token: CancellationToken) -> Result<()> {
    let HardwareContext {
        peripherals,
        mut threads,
        init_subsystems,
        run_ui,
    } = hardware_context;

    threads.push(supervised_thread("runtime", shutdown_token.clone(), {
        let shutdown_token = shutdown_token.clone();
        move || run_async_runtime(peripherals, init_subsystems, shutdown_token)
    })?);

    let result = run_ui();
    shutdown_token.cancel();
    let threads_had_errors = unwrap_threads(threads);

    result?;

    if threads_had_errors {
        bail!("At least one thread failed");
    }

    Ok(())
}

struct RuntimeWithInstantShutdown(Option<Runtime>);

impl RuntimeWithInstantShutdown {
    pub fn new() -> Self {
        Self(Some(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        ))
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.0.as_ref().unwrap().block_on(future)
    }
}

impl Drop for RuntimeWithInstantShutdown {
    fn drop(&mut self) {
        self.0.take().unwrap().shutdown_background()
    }
}
