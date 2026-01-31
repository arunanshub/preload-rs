#![forbid(unsafe_code)]

use tokio_util::sync::CancellationToken;

/// Install a Ctrl-C handler that cancels the provided token.
pub fn install_ctrl_c(cancel: CancellationToken) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel.cancel();
        }
    });
}
