use crate::signals::SignalEvent;
use flume::SendError;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to install signal handler: {0}")]
    SignalHandler(#[source] io::Error),

    #[error("Failed to send signal event: {0}")]
    SendSignal(#[from] SendError<SignalEvent>),
}
