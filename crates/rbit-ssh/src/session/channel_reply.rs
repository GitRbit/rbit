//! Bounded channel replies tracked by the SSH server owner.

use super::{FAILURE, PendingChannel, STDERR};
use russh::{ChannelId, server::Session};
use tokio::io::AsyncWriteExt;

/// Accepts a request, shows `message` on the client's stderr and ends the channel with
/// `status`. Used for refusals, so the client sees *why* rather than a bare failure.
pub(super) fn finish_with_message(session: &mut Session, channel: ChannelId, message: &str, status: u32) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    session.extended_data(channel, STDERR, message.as_bytes().to_vec())?;
    session.exit_status_request(channel, status)?;
    session.eof(channel)?;
    session.close(channel)
}

/// Accepts a request, writes `output` to stdout, and flushes before ending the channel.
pub(super) fn finish_pending_with_output(
    session: &mut Session,
    channel: ChannelId,
    pending: PendingChannel,
    output: Vec<u8>,
) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    let tasks = pending.tasks.clone();
    tasks.reply(async move {
        let PendingChannel { channel, _slot, .. } = pending;
        let (_input, writer) = channel.split();
        let mut stdout = writer.make_writer();
        let _ = stdout.write_all(&output).await;
        let _ = stdout.flush().await;
        let _ = writer.exit_status(0).await;
        let _ = writer.eof().await;
        let _ = writer.close().await;
    });
    Ok(())
}

/// Accepts a request, writes `message` to stderr, and flushes before ending the channel.
pub(super) fn finish_pending_with_message(
    session: &mut Session,
    channel: ChannelId,
    pending: PendingChannel,
    message: String,
    status: u32,
) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    let tasks = pending.tasks.clone();
    tasks.reply(async move {
        let PendingChannel { channel, _slot, .. } = pending;
        let (_input, writer) = channel.split();
        let mut stderr = writer.make_writer_ext(Some(STDERR));
        let _ = stderr.write_all(message.as_bytes()).await;
        let _ = stderr.flush().await;
        let _ = writer.exit_status(status).await;
        let _ = writer.eof().await;
        let _ = writer.close().await;
    });
    Ok(())
}

/// Ends a Git service with a flushed protocol error packet, so the client does not wait
/// for a ref advertisement that will never arrive.
pub(super) fn finish_with_git_error(
    session: &mut Session,
    channel: ChannelId,
    pending: PendingChannel,
    message: String,
) -> Result<(), russh::Error> {
    session.channel_success(channel)?;
    let tasks = pending.tasks.clone();
    tasks.reply(async move {
        let PendingChannel { channel, _slot, .. } = pending;
        let (_input, writer) = channel.split();
        let mut stdout = writer.make_writer();
        let packet = rbit_transport::pkt_line::error(&message);
        let _ = stdout.write_all(&packet).await;
        let _ = stdout.flush().await;
        let _ = writer.exit_status(FAILURE).await;
        let _ = writer.eof().await;
        let _ = writer.close().await;
    });
    Ok(())
}
