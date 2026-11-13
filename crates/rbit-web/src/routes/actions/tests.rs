use futures_util::StreamExt as _;

#[tokio::test]
async fn a_finished_download_body_can_be_polled_again() {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (completed, completion) = tokio::sync::oneshot::channel();
    sender.send(Ok(axum::body::Bytes::from_static(b"chunk"))).await.unwrap();
    drop(sender);
    completed.send(Ok(())).unwrap();
    let mut data = super::channel_body(receiver, completion).into_data_stream();
    assert_eq!(&data.next().await.unwrap().unwrap()[..], b"chunk");
    assert!(data.next().await.is_none());
    // An unfused stream panics here, which aborts a release build.
    assert!(data.next().await.is_none(), "polling a finished body again ends it again");
}

#[tokio::test]
async fn an_unread_artifact_releases_its_slot_and_still_reports_the_timeout() {
    use rbit_core::{
        RepoId, Timestamp,
        actions::{Artifact, ArtifactId, RunId},
    };
    use std::time::Duration;

    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("artifact.tar.gz");
    // Incompressible data fills the response queue for both archive formats.
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let noise: Vec<u8> = (0..(2 << 20) / 8)
        .flat_map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()
        })
        .collect();
    let mut stored = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    let mut header = tar::Header::new_gnu();
    header.set_size(noise.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    stored.append_data(&mut header, "w/noise.bin", noise.as_slice()).unwrap();
    let bytes = stored.into_inner().unwrap().finish().unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let artifact = Artifact {
        id: ArtifactId(1),
        run_id: RunId(1),
        repo_id: RepoId(1),
        attempt: 1,
        job_id: None,
        name: "noise".to_owned(),
        size: bytes.len() as u64,
        sha256: [0; 32],
        created_at: Timestamp(1),
        expires_at: Timestamp(2),
    };
    for format in [super::ArtifactFormat::TarGz, super::ArtifactFormat::Zip] {
        let slot = SLOTS.try_acquire().unwrap();
        let file = tokio::fs::File::open(&path).await.unwrap();
        let response = super::artifact_download_with_stall(&artifact, file, format, slot, Duration::from_millis(20));
        // Hold the real HTTP body unread. Worker error forwarding must not
        // wait on the same full channel that caused the write timeout.
        tokio::time::timeout(Duration::from_secs(5), async {
            while SLOTS.available_permits() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("a stalled response must release its archive admission");
        let mut data = response.into_body().into_data_stream();
        let mut failed = false;
        while let Some(chunk) = data.next().await {
            if chunk.is_err() {
                failed = true;
            }
        }
        assert!(failed, "a timeout must remain a broken download after buffered chunks are drained");
        assert!(data.next().await.is_none(), "failed bodies stay fused");
    }
}

#[tokio::test]
async fn an_archive_worker_panic_does_not_become_a_successful_body() {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (completed, completion) = tokio::sync::oneshot::channel::<std::io::Result<()>>();
    drop(sender);
    drop(completed);
    let mut data = super::channel_body(receiver, completion).into_data_stream();
    assert!(data.next().await.unwrap().is_err());
    assert!(data.next().await.is_none());
}

#[test]
fn a_client_that_stops_reading_releases_the_download() {
    use std::io::Write as _;
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let mut writer = super::ChunkWriter { stall: std::time::Duration::from_millis(50), ..super::ChunkWriter::new(sender) };
    writer.write_all(&vec![0; super::ChunkWriter::SIZE]).unwrap();
    // The channel holds one chunk and nobody reads it: the next one gives up.
    let stalled = writer.write_all(&vec![0; super::ChunkWriter::SIZE]).unwrap_err();
    assert_eq!(stalled.kind(), std::io::ErrorKind::TimedOut);
}
