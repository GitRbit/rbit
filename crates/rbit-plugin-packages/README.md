# Plugin package archives

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

## Host-owned archive staging

`PackageStore::open(root)` uses an existing, durably provisioned private root.
The host must retain exclusive write authority and keep the root and its
ancestors stable for every operation. The final root component must be a
nonsymlink directory. Unix roots must have no group or other permission bits;
on Windows the host must provision and protect the ACL because this API does
not verify it. These are deployment preconditions, not protection against an
actor able to replace ancestors or mutate private staging paths.

`store.stage(reader, control)` creates a private temporary directory beneath
that root and a normal, create-new archive file (mode `0600` on Unix). It tees
the validator's bounded reads into that file while hashing the whole archive.
Validation consumes actual EOF before the file is synchronized and returned as
a `PreparedPackage`. Publication uses this same staged inode: it does not
validate a source path and then reopen or copy potentially different bytes.
The prepared value exposes metadata and its archive digest, with no writable
handle or staging path. Payload files are never extracted.

`prepared.publish()` consumes that value and publishes
`<archive-sha256>.rbit-plugin` directly beneath the root. Plugin IDs, versions
and payload paths never become store filenames, including Windows reserved
names. Unix uses a no-clobber hard link, then synchronizes the root directory;
Windows uses `atomicwrites::move_atomic`, whose `MoveFileExW` operation requests
`MOVEFILE_WRITE_THROUGH` without replacement or cross-volume copying. The
private staging directory is on the same filesystem. Published objects are
logically immutable under the host's exclusive write authority; this API does
not set filesystem immutable attributes.

An existing target must be a nonsymlink regular file with exactly the expected
size and whole-archive SHA256 digest. A corrupt or mismatching target is never
overwritten. On Unix the verified existing file is synchronized before the
root directory, so its contents and namespace can be reported durable without
a prior publication receipt. On Windows an `ExistingVerified` receipt retains
`Durability::Unconfirmed`: file existence and matching bytes do not prove that
a previous namespace update completed durably. A future installation
coordinator must establish a prior durable ledger receipt or reconcile the
unreferenced object before claiming installation success.

`PackageReceipt` distinguishes `Created` from `ExistingVerified` and reports
observed durability. An error can occur after the object becomes visible:
`StoreFailure` separately reports publication visibility, an optional verified
receipt and whether private staging cleanup completed. `Unconfirmed`
visibility means the target may exist; it is not a rollback guarantee. A
receipt with unconfirmed durability must not be promoted into a successful
durable installation. Published objects are never rolled back after a sync or
cleanup error. `CleanupStatus::Pending` requires host reconciliation of private
staging remnants; dropping a prepared value only attempts best-effort cleanup,
while `discard()` reports cleanup failure explicitly.

`StageControl` supplies a finite deadline, a shared cancellation flag and a
wire quota no larger than `MAX_ARCHIVE_WIRE_BYTES`. Reserve one byte beyond the
archive length for the final EOF probe. Checks surround bounded streaming I/O
and occur before publication. Once publication succeeds, synchronization and
cleanup finish even if cancellation arrives. Blocking source reads, filesystem
operations and synchronization cannot be interrupted by these cooperative
checks. The caller must enforce transport/operation deadlines and retain its
owned admission lease until the operation returns; staging itself does not
provide global concurrency or disk admission policy.

This is an archive object store, not complete installation or recovery support.
A future coordinator must durably associate plugin IDs and versions with
objects, reconcile interrupted publication and private remnants, verify
publisher trust, enforce grants and lifecycle policy, and execute only through
an approved adapter. Staging writes no installation ledger or live intent and
grants no capabilities. A digest proves byte agreement, not publisher identity
or code safety.
