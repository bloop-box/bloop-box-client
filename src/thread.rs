use anyhow::Result;
use std::panic::UnwindSafe;
use std::thread::JoinHandle;
use std::{io, panic, thread};
use tokio_util::sync::CancellationToken;
use tracing::error;

pub struct SupervisedThread {
    name: String,
    join_handle: JoinHandle<Result<()>>,
}

/// Creates a supervised thread.
///
/// Any errors or panics are caught and returned through the join handle. When
/// a thread finishes, either successfully or with an error, the shutdown token
/// will be triggered, which will cause an application-wide shutdown.
pub fn supervised_thread<F>(
    name: impl Into<String>,
    shutdown_token: CancellationToken,
    f: F,
) -> Result<SupervisedThread, io::Error>
where
    F: FnOnce() -> Result<()> + UnwindSafe + Send + 'static,
{
    let name = name.into();

    let builder = thread::Builder::new().name(name.clone());
    let join_handle = builder.spawn({
        let name = name.clone();

        move || {
            let result = panic::catch_unwind(f);
            shutdown_token.cancel();

            if let Err(err) = result {
                let msg = if let Some(s) = err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                return Err(anyhow::anyhow!("Thread {name} panicked: {msg}"));
            }

            Ok(())
        }
    })?;

    Ok(SupervisedThread { name, join_handle })
}

/// Upwraps all threads and reports any errors.
pub fn unwrap_threads(threads: Vec<SupervisedThread>) -> bool {
    let mut has_errors = false;

    for thread in threads {
        // Safe to unwrap since this comes from supervised threads.
        let result = thread.join_handle.join().unwrap();

        if let Err(err) = result {
            error!("Thread {:?} failed: {:?}", thread.name, err);
            has_errors = true;
        }
    }

    has_errors
}
