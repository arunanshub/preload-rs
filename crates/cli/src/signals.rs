use crate::error::Error;
use flume::Sender;
use tokio::signal::unix::{signal, SignalKind};
use tracing::debug;

/// Indefinitely listens to signals and sends signal events to the provided channel.
#[tracing::instrument(skip_all)]
pub async fn wait_for_signal(signal_event: Sender<SignalEvent>) -> Result<(), Error> {
    let mut sigusr1 = signal(SignalKind::user_defined1()).map_err(Error::SignalHandler)?;
    let mut sigusr2 = signal(SignalKind::user_defined2()).map_err(Error::SignalHandler)?;
    debug!(?sigusr1, ?sigusr2, "Signal handlers registered");

    loop {
        tokio::select! {
            _ = sigusr1.recv() => {
                signal_event.send_async(SignalEvent::DumpStateInfo).await?;
            }
            _ = sigusr2.recv() => {
                signal_event.send_async(SignalEvent::ManualSaveState).await?;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SignalEvent {
    /// Dump the current state and related statistics.
    DumpStateInfo,
    /// Manually save the current state.
    ManualSaveState,
}
