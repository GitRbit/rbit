//! The fork-run regression group of `docs/design/rewrite/security-contracts.md`: runs of a
//! fork's pull request receive no secret or variable and only a read-only task token, also
//! once approved and with their author trusted; an approval holds for the run it was given
//! only; and a job is completed, logged, and given artifacts and caches only by the runner
//! holding its current lease (`docs/design/actions.md` §11.1, §11.11, §12.6).

mod approval;
mod leases;
mod reach;
mod uploads;
mod world;
