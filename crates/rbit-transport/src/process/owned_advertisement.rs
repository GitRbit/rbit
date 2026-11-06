//! Admitted advertisement work owns cleanup even when its HTTP waiter disappears.

use std::{io, path::Path, process::Stdio};
use tokio::{process::Child, sync::oneshot};
use tokio_util::sync::CancellationToken;

use super::{ProcessControl, ProcessTransport, ReceiveOptions, TransportError, advertisement, is_safe_protocol_header};
use crate::{Service, pkt_line};

impl ProcessTransport {
    /// Runs an HTTP advertisement with caller-owned admission retained by its owner.
    ///
    /// Dropping this future cancels its independent owner; deadline expiry is handled
    /// cooperatively there. Neither releases the guard before awaited child/pipeline
    /// cleanup. The host must supply real admission before calling: one owner is
    /// spawned per admitted operation, not an unbounded request or fallback queue.
    ///
    /// # Errors
    /// Returns controlled/spawn/I/O failures or a nonzero Git status. An I/O wait
    /// failure or owner panic is not proof of process exit or a global drain.
    pub async fn advertise_refs_owned<G: Send + 'static>(
        &self,
        service: Service,
        repo_path: &Path,
        protocol: Option<&str>,
        control: ProcessControl,
        guard: G,
    ) -> Result<Vec<u8>, TransportError> {
        control.check()?;
        let child = self
            .command(service, repo_path, protocol, ReceiveOptions::default())
            .arg("--advertise-refs")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(TransportError::Spawn)?;
        let advertisement = owned(service, child, control, guard).await?;
        let is_v2 = protocol.filter(|p| is_safe_protocol_header(p)).is_some_and(|p| p.split(':').any(|kv| kv == "version=2"));
        let mut body = Vec::with_capacity(advertisement.len() + 64);
        if !is_v2 {
            body.extend(pkt_line::encode(&format!("# service={}\n", service.wire_name())));
            body.extend_from_slice(pkt_line::FLUSH);
        }
        body.extend(advertisement);
        Ok(body)
    }
}

async fn owned<G: Send + 'static>(service: Service, child: Child, control: ProcessControl, guard: G) -> Result<Vec<u8>, TransportError> {
    let (control, request) = control.request_scope();
    let _cancel_on_drop = CancelRequest(request);
    let (done, completion) = oneshot::channel();
    tokio::spawn(async move {
        let result = advertisement::collect(service, child, Some(&control)).await;
        drop(guard);
        let _ = done.send(result);
    });
    completion.await.map_err(|_| TransportError::Io(io::Error::other("Git advertisement owner ended without reporting completion")))?
}

struct CancelRequest(CancellationToken);

impl Drop for CancelRequest {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(test)]
mod tests;
