# Phase 00 — safe project setup

## Scope and authorization

The owner explicitly requested `PRD.md` first, then setup/scaffolding. Save the
requirements, roadmap, research ledger, architecture boundaries, protocol unknowns,
test matrix, and dependency-light development diagnostics. Start primary-source
research and official installer acquisition/static inspection as available.

Python standard-library tooling is for diagnostics only; it does not select the
runtime architecture. Do not create a guessed protocol parser, fake UHID daemon,
active udev rule, enabled service, or skeleton advertised as working support.
No firmware writes, Windows execution, grabs, hidraw output, or device suppression.

## Deliverables

- Root `PRD.md`, `README.md`, `PLAN.md`, `PROGRESS.md`.
- `docs/RESEARCH.md`, `PROTOCOL.md`, `ARCHITECTURE.md`, `TESTING.md`,
  `OPEN_QUESTIONS.md`, phase records, and evidence retention instructions.
- Read-only sysfs inventory tool with redacted device identities and offline
  tests; no `/dev/hidraw*` reads/writes or permission changes.
- Reproducible acquisition instructions and external artifact identity record;
  no proprietary binary or decompilation committed.
- Continuous offline test entry point; runtime service/udev deployment explicitly
  deferred until the translator and device matching are safe.

## Acceptance checks

| Criterion | Check and observed evidence | Result |
| --- | --- | --- |
| S1 Requirements retained | Independent requirement trace and `test_documentation` local-link/file checks passed; F1–F5/A1–A11 retained | PASS |
| S2 Safe diagnostics | Complete tool inspection and offline safety/error/redaction tests; no device-node access or target writes | PASS |
| S3 Runnable scaffold | `python3 -m unittest discover -s tests -v`: 17 passed on Python 3.14.8, including both descriptor-fixture subtests; CLI help and initial inventory exit 0; 4/4 Python diagnostic reports, 0 errors/warnings | PASS |
| S4 Evidence hygiene | Independent artifact-ignore/privacy/whitespace checks and external installer/descriptor hash comparisons passed; raw artifacts outside Git | PASS |
| S5 Honest status | Independent verification and same-version convergence passed; all functional product criteria remain untested | PASS |

No full-runtime or hardware criterion is a Phase 00 exit condition. Source/site
unavailability is recorded with exact remaining research, not treated as a passed
research gate.

## Baseline

Empty unborn `main`; Python 3.14.8; no tests or preexisting repository work.
Planning write permission failure is a tooling limitation, not a product failure.

## Results

**Phase 00 complete.** Independent verifier and convergence assessed the same
24-file implementation/evidence version, manifest SHA-256
`193b5220af6bbee398ac4195f762db7e625cde583d8536a79764f8207c3dc248`.
Manifest: sorted `{path, bytes, sha256}` records serialized as compact key-sorted
JSON. These factual closeout updates follow that assessment; no tool behavior or
fixtures changed afterward. No setup blockers remain.

Xhigh review was not warranted for this metadata-only development scaffold;
runtime/identity/security configuration choices remain for later review. The
scribe session could not write under its permissions, so the parent saved its
verified factual updates. Phase 01/product acceptance are **not complete**.

See `../../PROGRESS.md` for remaining work and `../TESTING.md` for exact checks.
