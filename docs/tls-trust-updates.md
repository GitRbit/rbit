# TLS trust patches

The lockfile updates `rustls-webpki` from 0.103.15 to 0.103.16 and `webpki-roots` from 1.0.9 to 1.0.10. These are the patched versions named by [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382) and [RUSTSEC-2026-0381](https://rustsec.org/advisories/RUSTSEC-2026-0381).

The patches reject certificate paths through distrusted emSign intermediate keys and remove the affected root from the bundled root store. No settings or database changes are needed. The advisory check must pass before release; the update does not add an advisory exception.
