# Space Station 3.4.4.3 acquisition record

## Identity and trust status

| Field | Observation |
| --- | --- |
| Candidate URL supplied by owner | `https://tencent-android.cdn.flydigi.com/PC/Space%20Station3.0/FlydigiSpaceStation_setup_3.4.4.3_Release.exe` |
| Official page | `https://shops.flydigi.com/pages/game-center` returned HTTP 429 during setup |
| CDN availability | HTTPS HEAD and download succeeded; HTTP 200 |
| Byte length | `105741288` |
| SHA-256 | `f6e480b75016288797080ba5257aa8cbc46ebc724401ee841680f8c755ca0d50` |
| Local artifact | `/tmp/opencode/flydigi/official/FlydigiSpaceStation_setup_3.4.4.3_Release.exe` (outside Git, temporary storage) |
| Format | x86 PE32; 7-Zip identifies Inno Setup, product resource 3.4.4.3 |
| Authenticode | **Unverified**: certificate table observed, `osslsigncode` unavailable; existence of a certificate is not a valid signature/trust chain |
| Provenance conclusion | Owner-supplied Flydigi-domain candidate is operational; current official-page linkage and publisher authentication are unresolved |
| Execution | None; downloaded/listed/parsed only |

The hash identifies the acquired bytes. It is not an independently published
vendor checksum and does not itself authenticate the publisher.

## Reproduce acquisition without execution

Use a new private directory outside the checkout; do not overwrite a prior
artifact. Requires curl and sha256sum; these commands are not a runtime install.

```sh
mkdir -p "$HOME/.local/share/flydigi-research"
artifact_dir="$(mktemp -d "$HOME/.local/share/flydigi-research/3.4.4.3.XXXXXX")"
curl --fail --location --proto '=https' --proto-redir '=https' --max-time 300 \
  --output "$artifact_dir/FlydigiSpaceStation_setup_3.4.4.3_Release.exe" \
  'https://tencent-android.cdn.flydigi.com/PC/Space%20Station3.0/FlydigiSpaceStation_setup_3.4.4.3_Release.exe'
sha256sum "$artifact_dir/FlydigiSpaceStation_setup_3.4.4.3_Release.exe"
7z l "$artifact_dir/FlydigiSpaceStation_setup_3.4.4.3_Release.exe"
```

Compare the resulting hash/length to the retained identity above. A mismatch is
a new artifact requiring investigation, not automatic proof of maliciousness
or permission to replace the saved evidence. Discard/reacquire interrupted
downloads in a new directory. Never run the EXE through Wine or Windows here.

## Static analysis checkpoint

- REA tool: `inspect_pe_resources`, resource parser version 1,
  `max_file_bytes=134217728`. Complete within PE resource inventory: 42 entries,
  18 resources; non-icon payloads are opaque to this tool.
- Evidence ID:
  `ev_e63ebd351d469ebc6798b6b5a6f6525d1bad34c56e91ff1517678afeb1407b1a`.
- Export: `/tmp/opencode/flydigi/rea-evidence-setup.json`, 14456 bytes, one record.
- Supporting `7z l` (7-Zip 26.05) sees PE sections, certificate and overlay,
  **not extracted application assemblies**. PE metadata is self-reported.
- `innoextract`, `osslsigncode`, `dotnet`, and `ilspycmd` were not found. No
  unrelated packages or MCP configuration were installed/changed.

## Remaining work

Recheck official-page linkage when accessible. Use an available trusted
Authenticode verifier (record signer, digest, chain/timestamp trust separately).
Extract Inno contents with a suitable static extractor into a new external
directory; then use REA `inspect_managed_artifact` and focused member/CIL tools
on the relevant assemblies. Preserve their hashes and method/token locations.
Trace only the authorized controller protocol paths. Never publish binaries or
substantial proprietary source. Preserve the temporary evidence elsewhere before
cleanup/reboot if needed; Git does not back it up.
