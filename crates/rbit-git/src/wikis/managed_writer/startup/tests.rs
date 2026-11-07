use super::*;
use std::io::Write;

fn compressed(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut writer = gix::zlib::stream::deflate::Write::new(&mut output, gix::zlib::Compression::DEFAULT);
    writer.write_all(bytes).unwrap();
    writer.flush().unwrap();
    drop(writer);
    output
}
fn id(bytes: &[u8]) -> gix::ObjectId {
    gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, bytes).unwrap()
}

#[test]
fn bounded_loose_decoder_refuses_wrong_kind_size_hash_checksum_truncation_and_trailing_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("loose");
    let expected = id(b"body");
    std::fs::write(&path, compressed(b"blob 4\0body")).unwrap();
    let decoded = loose::read(&path, expected).unwrap();
    assert_eq!(&decoded.data[decoded.offset..], b"body");
    let mut trailing = compressed(b"blob 4\0body");
    trailing.push(0);
    let mut truncated = compressed(b"blob 4\0body");
    truncated.pop();
    let mut checksum = compressed(b"blob 4\0body");
    let last = checksum.len() - 1;
    checksum[last] ^= 1;
    for bytes in [
        compressed(b"blob 5\0body"),
        compressed(b"blob 04\0body"),
        compressed(b"tag 4\0body"),
        compressed(b"blob 4\0evil"),
        trailing,
        truncated,
        checksum,
        compressed(b"blob 1048577\0small"),
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(loose::read(&path, expected).is_err());
    }
}

#[test]
fn pending_tree_uses_strict_numeric_ordered_unique_plain_blob_entries() {
    for names in [vec!["1.md", "1.md"], vec!["2.md", "1.md"], vec!["01.md"], vec!["../x"], vec!["9223372036854775808.md"]] {
        let mut data = Vec::new();
        for name in names {
            data.extend_from_slice(b"100644 ");
            data.extend_from_slice(name.as_bytes());
            data.push(0);
            data.extend_from_slice(id(b"body").as_slice());
        }
        let object = gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Tree, &data).unwrap();
        let mut pending =
            pending::Pending::new(object, loose::Decoded { data, offset: 0, kind: rbit_wikis::startup::objects::WikiScannedKind::Tree })
                .unwrap();
        let refused = loop {
            match pending.next() {
                Err(_) => break true,
                Ok(None) => break false,
                Ok(Some(_)) => {}
            }
        };
        assert!(refused);
    }
}
