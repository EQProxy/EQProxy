# Evidence

This directory holds the recorded runs behind the numbers and behaviour
claims in the README and in [`../parity-and-evidence.md`](../parity-and-evidence.md).
Each run is preserved with the commands, configuration, logs and raw data
that produced its result; later changes do not rewrite those records.

## TLS material

Test TLS and mTLS private keys are generated at build time into
`OUT_DIR/fixtures` and are never committed. The historical benchmark records
keep their public certificates (`cert.pem`), but their private key halves
(`key.pem` and the combined `haproxy.pem`) were removed from the tracked
tree on 2026-11-04.

A reader reproducing a run must generate ephemeral keys for the certificate
in the run's `certs/` directory; the recorded certificate fingerprints and
measurements remain the ones from the original run. Documents that cite
`eqproxy-net/tests/fixtures/*.pem` describe the layout before that date.

CI rejects private keys in tracked files, with no allowlist, and the release
workflow rejects a source tarball that contains one.
