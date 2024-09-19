use crate::error::Error;
use flume::Sender;
use tokio::signal::unix::{signal, SignalKind};

/// Indefinitely listens to signals and sends signal events to the provided channel.
pub async fn wait_for_signal(signal_event: &Sender<SignalEvent>) -> Result<(), Error> {
    let mut sigusr1 = signal(SignalKind::user_defined1()).map_err(Error::SignalHandler)?;
    let mut sigusr2 = signal(SignalKind::user_defined2()).map_err(Error::SignalHandler)?;

    loop {
        tokio::select! {
            _ = sigusr1.recv() => {
                signal_event.send_async(SignalEvent::SigUSR1).await?;
            }
            _ = sigusr2.recv() => {
                signal_event.send_async(SignalEvent::SigUSR2).await?;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SignalEvent {
    SigUSR1,
    SigUSR2,
}
