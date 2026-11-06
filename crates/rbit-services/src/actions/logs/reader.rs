//! Indexed plaintext/gzip window reader; no authorization or database access.
use super::{LogRange, LogWindow, types::LogFiles};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

/// The end offsets of stored lines `from..to` (fewer when the index is shorter).
fn read_ends(index: &mut File, from: u64, to: u64) -> std::io::Result<Vec<u64>> {
    let mut ends = Vec::new();
    if to <= from {
        return Ok(ends);
    }
    index.seek(SeekFrom::Start(from * 8))?;
    let mut bytes = Vec::new();
    index.take((to - from) * 8).read_to_end(&mut bytes)?;
    ends.extend(bytes.as_chunks::<8>().0.iter().map(|chunk| u64::from_le_bytes(*chunk)));
    Ok(ends)
}

/// Fills `window` with lines `start..end` of the log, chosen by `range` and the limits.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_window(
    files: &LogFiles,
    gzip: bool,
    start: u64,
    end: Option<u64>,
    range: LogRange,
    max_lines: usize,
    max_bytes: usize,
    window: &mut LogWindow,
) -> std::io::Result<()> {
    let mut index = match File::open(&files.index) {
        Ok(index) => index,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let stored = index.metadata()?.len() / 8;
    let end = end.map_or(stored, |end| end.min(stored));
    let max = u64::try_from(max_lines).unwrap_or(u64::MAX);
    let (first, last) = match range {
        LogRange::From(line) => {
            let first = line.clamp(start, end.max(start));
            (first, end.min(first.saturating_add(max)))
        }
        LogRange::Tail => (end.saturating_sub(max).max(start), end),
    };
    window.first_line = first;
    window.next_line = first;
    if first >= last {
        window.more_before = first > start;
        return Ok(());
    }
    // The end of the line before `first` is where `first` begins.
    let ends = read_ends(&mut index, first.saturating_sub(1), last)?;
    let (begin, ends) =
        if first == 0 { (0, &ends[..]) } else { (ends.first().copied().unwrap_or_default(), ends.get(1..).unwrap_or_default()) };
    let budget = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let (first, begin, ends) = match range {
        LogRange::From(_) => {
            let count = ends.iter().take_while(|end| **end - begin <= budget).count().max(1).min(ends.len());
            (first, begin, &ends[..count])
        }
        LogRange::Tail => {
            let finish = ends.last().copied().unwrap_or(begin);
            let start_of = |line: usize| if line == 0 { begin } else { ends[line - 1] };
            let mut skip = 0;
            while skip + 1 < ends.len() && finish - start_of(skip) > budget {
                skip += 1;
            }
            (first + u64::try_from(skip).unwrap_or_default(), start_of(skip), &ends[skip..])
        }
    };
    let finish = ends.last().copied().unwrap_or(begin);
    let mut bytes = vec![0u8; usize::try_from(finish - begin).unwrap_or_default()];
    if gzip {
        let mut decoder = flate2::read::GzDecoder::new(File::open(&files.gzip)?);
        std::io::copy(&mut (&mut decoder).take(begin), &mut std::io::sink())?;
        decoder.read_exact(&mut bytes)?;
    } else {
        let mut log = File::open(&files.log)?;
        log.seek(SeekFrom::Start(begin))?;
        log.read_exact(&mut bytes)?;
    }
    let mut offset = begin;
    for end in ends {
        let from = usize::try_from(offset - begin).unwrap_or_default();
        let to = usize::try_from(*end - begin).unwrap_or_default();
        let line = bytes.get(from..to).unwrap_or_default();
        window.lines.push(String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(line)).into_owned());
        offset = *end;
    }
    window.first_line = first;
    window.next_line = first + u64::try_from(ends.len()).unwrap_or_default();
    window.more_before = first > start;
    Ok(())
}
