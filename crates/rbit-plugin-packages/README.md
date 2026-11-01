# Plugin package archive validation

`validate_archive(Read)` validates an entire uncompressed archive and returns its
`PackageManifest` and byte counts. It writes no files, starts no processes and
makes no trust or capability decision. Hosts must enforce transport deadlines.
It is a prerequisite for an installer, not an installer.

The supported profile is intentionally narrower than general tar:

- Every header is a 512-byte POSIX USTAR header with `ustar\0` magic and `00`
  version. Only explicit regular-file type flag `0` is accepted. The first file
  is exactly `package.json`, with 1 through 1,048,576 bytes of strict manifest
  JSON. Declared payloads cannot use that path in any ASCII case, or descend
  from it. Every payload appears exactly once, in any order, with its exact
  declared spelling, size and SHA256 digest. No undeclared files are allowed.
- Name and prefix fields contain ASCII portable paths from `PackagePath`.
  Bytes following their first NUL must all be zero. The USTAR prefix is joined
  with the name before path validation; no filesystem or tar normalization is
  performed. Paths must be representable by the 100-byte name and 155-byte
  prefix fields, in addition to the manifest's path limits.
- Numeric fields use zero-padded ASCII octal digits followed by a single NUL:
  7 digits for mode, uid, gid and checksum; 11 for size and mtime. The checksum
  is the unsigned sum of all header bytes, treating its own eight bytes as
  spaces. Space-padded numbers, the common six-digit/NUL/space checksum form,
  and GNU base-256 numbers are outside this profile.
- Mode is `0644` or `0755`; uid, gid and mtime are zero. Device fields are all
  NUL or the canonical octal encoding of zero. Link name, owner/group names
  and reserved header bytes are all zero. These fields confer no authority;
  validation never applies ownership, permissions or timestamps.
- Payload body padding is all zero. Exactly two all-zero 512-byte blocks end
  the archive, followed immediately by actual EOF. Extra record padding,
  concatenated archives and any trailing byte are rejected. `tar::Header`
  `new_ustar`, explicit mode/uid/gid/mtime, and `set_cksum` can produce the
  accepted header encoding; packagers must still follow the exact envelope.
- Directories, links, devices, sparse files, GNU/PAX extension records, old tar
  headers, compressed streams and unknown header metadata are rejected.

Manifest bytes are capped before allocation and JSON deserialization. Payloads
are hashed through a fixed 64 KiB buffer. Manifest/file counts, individual and
aggregate sizes come from `rbit-plugin-api`; `MAX_ARCHIVE_WIRE_BYTES` additionally
bounds all headers, padding, terminators and the single-byte trailing-data probe.
Diagnostics are static and do not print arbitrary JSON keys or payload data.

The validator deliberately reads the envelope itself. In tar 0.4.46, even
`entries().raw(true)` terminates at the first zero header (or bare EOF) and does
not enforce a second zero block, zero padding or actual EOF. Default iteration
also interprets GNU/PAX extensions. No tar extraction API is used here.

A future installer must separately stage immutable bytes, handle filesystem
races and platform rules, verify publisher trust, enforce grants and lifecycle
policy, and execute only through an approved adapter. A digest proves agreement
with this descriptor, not the publisher's identity or the safety of code.
