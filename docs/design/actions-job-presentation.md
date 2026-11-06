# Production Job detail presentation

The job page groups its real state, step count, runner labels and available duration
in the platform's existing box, typography and semantic color tokens. Long matrix
and step names wrap on small screens. Native details summaries remain keyboard
controls; raw download and writer-authorized native rerun POSTs keep their existing
URLs. Run navigation keeps cancellation, artifacts and run-level controls at their
existing destination. No graph, synthetic failure explanation, new artifact query,
search control or live-follow API is introduced.

Job and step log models live in `views/actions/job.rs`, with small header/steps
includes. The route and service acquisition code are unchanged: one authorized
batched job read, original indexed shared decode and cooperative rendering yields.
Previews retain 1000 lines / 256 KiB per step and the existing byte-boundary overshoot
contract; step pages and full raw downloads keep their separate existing limits.
Line HTML, ANSI removal, blocks, anchors and virtual-row markup are unchanged.
Retention expiry, no output, an unstarted step, omitted earlier lines and producer
truncation remain distinct states.

Typed compiled job/log messages are prepared once per document. Static English
labels, including row state labels, borrow catalog literals; counts write directly
to the formatting destination in English. Pseudo locales use the same typed catalog.
Both selected refresh roots carry their own `lang` and `dir`; private cache/Vary
semantics remain in the existing locale middleware. No JavaScript source or bundle
changes are needed, and the existing step-open preservation logic is unchanged.

Fixtures exercise real runner output across the 1000-line boundary, original line
numbers, larger step views and byte-equal raw downloads; finished writer/reader
rerun controls; and localized full/HTMX requests with equal actual SQLite statement
and acquisition counts. Source contracts pin the untouched line-rendering program.
These fixtures require the shared compiler gate. Source checks alone do not prove
Rust rendering, real HTMX behavior, visual quality or accessibility acceptance.
Fresh same-revision assets and isolated Linux browser review remain necessary.
