//! The hostile load regression group (`docs/design/rewrite/security-contracts.md`):
//! parser expansion and depth, diff, blame and highlighting work, log floods and
//! password work stay bounded under concurrent hostile requests through the real router,
//! and an ordinary read issued meanwhile is still served.

mod blame;
mod fixture;
mod logs;
mod passwords;
