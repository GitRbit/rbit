//! Buffered advertisements use the same kill/reap completion contract as streaming.

use super::{ProcessControl, TransportError, control, exchange::terminate};
use crate::Service;
use tokio::{io::AsyncReadExt, process::Child};

pub(super) async fn collect(service: Service, mut child: Child, control: Option<&ProcessControl>) -> Result<Vec<u8>, TransportError> {
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    let result = tokio::select! {
        biased;
        reason = control::stopped(control) => Err(reason),
        result = async { tokio::try_join!(stdout.read_to_end(&mut output), stderr.read_to_end(&mut errors), child.wait()) } => {
            result.map(|(_, _, status)| status).map_err(TransportError::Io)
        }
    };
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            terminate(&mut child).await?;
            return Err(error);
        }
    };
    if !status.success() {
        tracing::warn!(%service, stderr = %String::from_utf8_lossy(&errors), "ref advertisement failed");
        return Err(TransportError::Failed { service, status });
    }
    Ok(output)
}
