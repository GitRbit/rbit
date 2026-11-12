//! Download conversion acceptance: paths, modes, skipped entries and ZIP64.
use std::io::Cursor;

use super::*;

fn tar_gz(files: &[(&str, &[u8], u32, tar::EntryType)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    for (name, data, mode, kind) in files {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(*kind);
        header.set_size(data.len() as u64);
        header.set_mode(*mode);
        header.set_mtime(1_700_000_000);
        if *kind == tar::EntryType::Symlink {
            header.set_link_name("target").unwrap();
        }
        // `append_data` would refuse the names the converter must skip itself.
        let bytes = name.as_bytes();
        header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
        header.set_cksum();
        builder.append(&header, *data).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn convert(tar: &[u8]) -> zip::ZipArchive<Cursor<Vec<u8>>> {
    let bytes = to_zip(Cursor::new(tar), Vec::new()).unwrap();
    zip::ZipArchive::new(Cursor::new(bytes)).unwrap()
}

#[test]
fn files_keep_their_paths_contents_and_safe_modes() {
    use tar::EntryType::{Directory, Regular, Symlink};
    let tar = tar_gz(&[
        ("./w/a.txt", b"ay", 0o644, Regular),
        ("w/nested/", b"", 0o755, Directory),
        ("w/nested/deep/run.sh", b"#!/bin/sh\necho hi\n", 0o4755, Regular),
        ("w/link", b"", 0o777, Symlink),
        ("w/../escape", b"no", 0o644, Regular),
        ("/w/abs", b"no", 0o644, Regular),
        ("h/home", b"no", 0o644, Regular),
        ("w", b"no", 0o644, Regular),
        ("w/empty", b"", 0, Regular),
    ]);
    let mut zip = convert(&tar);
    let names: Vec<String> = zip.file_names().map(|name| name.unwrap().into_owned()).collect();
    assert_eq!(names, ["a.txt", "nested/deep/run.sh", "empty"]);
    let mut text = String::new();
    zip.by_name("nested/deep/run.sh").unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(text, "#!/bin/sh\necho hi\n");
    assert_eq!(zip.by_name("nested/deep/run.sh").unwrap().unix_mode(), Some(0o100_755), "set-user-ID is dropped");
    assert_eq!(zip.by_name("empty").unwrap().unix_mode(), Some(0o100_644));
    assert_eq!(zip.by_name("a.txt").unwrap().size(), 2);
}

#[test]
fn many_entries_use_zip64_end_records() {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    for index in 0..70_000 {
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, format!("w/f/{index}"), &b"x"[..]).unwrap();
    }
    let tar = builder.into_inner().unwrap().finish().unwrap();
    let mut zip = convert(&tar);
    assert_eq!(zip.len(), 70_000);
    let mut text = String::new();
    zip.by_name("f/69999").unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(text, "x");
}

#[test]
fn an_empty_or_broken_archive() {
    assert_eq!(convert(&tar_gz(&[])).len(), 0);
    let mut broken = tar_gz(&[("w/a", b"0123456789", 0o644, tar::EntryType::Regular)]);
    broken.truncate(broken.len() - 8);
    assert!(to_zip(Cursor::new(&broken), Vec::new()).is_err());
    assert!(to_tar_gz(Cursor::new(&broken), Vec::new()).is_err());
}

#[test]
fn tar_gz_downloads_have_the_files_at_their_top() {
    use tar::EntryType::{Regular, Symlink};
    let stored = tar_gz(&[("w/dist/a.zip", b"zip", 0o6755, Regular), ("w/link", b"", 0o777, Symlink), ("h/x", b"no", 0o644, Regular)]);
    let bytes = to_tar_gz(Cursor::new(stored), Vec::new()).unwrap();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(bytes)));
    let entries: Vec<_> = archive
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            (entry.path().unwrap().display().to_string(), entry.header().mode().unwrap(), text)
        })
        .collect();
    assert_eq!(entries, [("dist/a.zip".to_owned(), 0o755, "zip".to_owned())]);
}

// Actions: run logs.
#[test]
fn files_become_a_zip() {
    let mut zip = ZipWriter::new(Vec::new(), 1_700_000_000);
    zip.add("0_build.txt", 10, &mut &b"compiling\n"[..]).unwrap();
    zip.add("1_test.txt", 0, &mut io::empty()).unwrap();
    let bytes = zip.finish().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert_eq!(archive.len(), 2);
    let mut text = String::new();
    archive.by_name("0_build.txt").unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(text, "compiling\n");
}

#[test]
fn dos_times() {
    assert_eq!(dos_time(0), (0, 1 << 5 | 1), "before 1980 is 1980-01-01");
    // 2023-11-14T22:13:20Z
    assert_eq!(dos_time(1_700_000_000), ((22 << 11) + (13 << 5) + 10, (43 << 9) + (11 << 5) + 14));
}

#[cfg(unix)]
#[test]
fn a_file_with_holes_packed_as_sparse_comes_out_whole() {
    // Older runners packed files with `tar`'s default `sparse(true)`: a binary whose linker
    // left holes became a GNU sparse entry, and converting the artifact dropped it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rbit");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(4 << 20).unwrap();
    std::os::unix::fs::FileExt::write_all_at(&file, b"end", (4 << 20) - 3).unwrap();
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    builder.sparse(true);
    builder.append_path_with_name(&path, "w/rbit").unwrap();
    let tar = builder.into_inner().unwrap().finish().unwrap();
    let mut zip = convert(&tar);
    let mut entry = zip.by_name("rbit").unwrap();
    let mut data = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut data).unwrap();
    assert_eq!(data.len(), 4 << 20);
    assert_eq!(&data[data.len() - 3..], b"end");
    assert!(data[..1024].iter().all(|byte| *byte == 0));
}

#[test]
fn files_of_one_name_in_two_directories_are_refused_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let file = tar::EntryType::Regular;
    let clash = tar_gz(&[("w/linux/app", b"linux build", 0o755, file), ("w/macos/app", b"macos build", 0o755, file)]);
    let refused = extract_files(Cursor::new(clash), dir.path(), 20).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::InvalidData);
    assert!(refused.to_string().contains("`app`"), "{refused}");
    let apart = tar_gz(&[("w/linux/app-linux", b"linux build", 0o755, file), ("w/macos/app-macos", b"macos build", 0o755, file)]);
    let names: Vec<String> = extract_files(Cursor::new(apart), dir.path(), 20).unwrap().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["app-linux", "app-macos"]);
}
