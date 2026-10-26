# Evidence retention

Keep distributable protocol facts, upstream commit IDs, artifact SHA-256 hashes,
REA Evidence IDs, tool versions, limitations, and test results in project docs.
Never commit installers, DLLs, proprietary decompilation, raw captures, Bluetooth
addresses/serials, home paths, authentication data, or unrelated device inventory.

Suggested local analysis root: `/tmp/opencode/flydigi/` (outside this repository;
temporary and not archival). For durable work copy artifacts to a private directory
outside the repository, for example `$HOME/.local/share/flydigi-research/`.
Record the actual retained location in a private handoff, not captured personal
identifiers in published files. `.gitignore` is a safety net, not authorization
to publish or a replacement for checking the Git file list.

Each evidence entry should contain:

```text
claim | observed / inferred / hypothesis / unknown
source URL + immutable revision, or artifact SHA-256
transport + hardware/firmware/Steam version where relevant
method/token/file/line or capture event; REA Evidence ID
exact action/tool version | coverage limitations | next probe
```

Sanitized input fixtures require provenance, transport, firmware, descriptor hash,
expected state, and explicit synthetic/recorded labeling. Check sanitization
manually before committing; a redactor is not a privacy guarantee.
