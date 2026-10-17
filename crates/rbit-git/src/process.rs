//! Bounded capture of Git child output: drain both pipes without retaining remote-sized
//! diagnostics, and kill the child if reading fails or the caller cancels the operation.

use std::{
    io,
    process::{Output, Stdio},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

pub(crate) async fn bounded_output(command: &mut Command, stdout_limit: usize, stderr_limit: usize) -> io::Result<Output> {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| io::Error::other("git has no stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| io::Error::other("git has no stderr"))?;
    let (status, stdout, stderr) = tokio::try_join!(child.wait(), read_prefix(stdout, stdout_limit), read_prefix(stderr, stderr_limit))?;
    Ok(Output { status, stdout, stderr })
}

/// Consume the entire stream so the child cannot block on a full pipe, keeping only its
/// first `limit` bytes. This also bounds diagnostics with arbitrarily long lines.
async fn read_prefix(mut input: impl AsyncRead + Unpin, limit: usize) -> io::Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut buffer = vec![0_u8; 8192];
    loop {
        let read = input.read(&mut buffer).await?;
        if read == 0 {
            return Ok(retained);
        }
        let keep = read.min(limit.saturating_sub(retained.len()));
        retained.extend_from_slice(&buffer[..keep]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn diagnostics_are_bounded_and_the_entire_pipe_is_drained() {
        let data = vec![b'x'; 256 * 1024];
        for limit in [0, 500, 8192, data.len() + 1] {
            let mut input = data.as_slice();
            let retained = read_prefix(&mut input, limit).await.unwrap();
            assert_eq!(retained, data[..limit.min(data.len())]);
            assert!(input.is_empty(), "discarded output must still be drained");
        }
    }
}
