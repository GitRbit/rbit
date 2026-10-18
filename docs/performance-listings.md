# Bounded repository listings

The repository pages build on the earlier performance follow-ups (`a71a317`). This
follow-up reduces server allocations, repeated Git work, HTML and DOM size before
block rendering. It keeps the branch graph's List/Git tree toggle and the language
dialog's folder outline, and bounds the work behind both.

## Directory metadata

- A page renders at most 100 entries. Search precedes slicing; the original tree
  ordering and original entry indexes are preserved. Page and metadata URLs name
  the resolved commit so they refer to the same snapshot.
- Metadata requests compute only that page's entries. Their cache keys include the
  repository ID, commit ID, directory and sorted entry names. Repository access is
  checked on every request; permissions and readers are not stored in the cache.
- Results are reused for 30 seconds, including timeout results. There are at most
  32 slots, with a 512 KiB estimated key/result admission budget per slot. Larger
  results are served without retention. At most four workers are admitted; they
  own their permits even after the requesting page is cancelled.
- Duplicate requests share retained results. A failed job is retried. Incomplete
  results are never persisted as complete, and a page cannot populate the existing
  whole-directory persistent cache. HTTP immutable caching requires every requested
  cell to be resolved. Background full-directory warming retains its original API.
- Cache-hit filtering uses a set. Each walk retains at most 16 decoded directory
  trees within a 4 MiB estimate, reusing a parent's entries when visiting it next.

## Lists and graph

Branches, tags, directories and language files have search and ordinary page links,
with at most 100 results per page. The branch/tag picker separately loads 50 options
on demand. Recent-ref ordering, the default branch's priority, annotated tags,
archive links and branch protection badges remain available. Ref timestamps are
still scanned to establish recency, but full commit summaries and protection
matching are restricted to the requested page.

The graph still shows up to 200 commits with merge edges and children before their
parents. Each row uses one compound SVG path for its lane segments. The graph reads
resolved branch tips rather than full messages for every branch. At most eight
branch labels are shown per commit; an additional-count link leads to the complete
branch listing. The language browser builds its folder outline from the requested
page of paths only, so a page holds at most 100 files plus their ancestor folders.
In the dialog, page links and search load the next page in place; without
JavaScript they are ordinary links and a GET form.

## Verification

The router regression creates 205 files and 105 tags in fresh local repositories.
It checks first/middle/last pages, out-of-range pages, search, pinned links, commit
slot alignment, cache isolation, language pages, graph rendering and private access.
Unit tests cover shared work, cancellation, expiry, failures, admission limits,
history semantics, ref order, graph geometry and bounded labels.

For a representative Git repository, run:

```powershell
cargo run -p rbit-web --example measure_listings -- PATH/TO/git.git OUTPUT_DIRECTORY
```

This copies refs/objects into temporary application data, records two in-process
sample rounds and serves an ephemeral loopback port for two minutes for browser
inspection. It never modifies the source repository or deploys the application.
The example targets Git's `master` branch. Its recorded build profile must accompany
any measurements; debug timings are not a production or release speed comparison.

Remaining work is release-profile deployment validation and sustained/concurrent
load measurements. The language classifier still walks the repository tree, and
recent-ref ordering still scans matching ref timestamps; pagination primarily bounds subsequent work and output.
