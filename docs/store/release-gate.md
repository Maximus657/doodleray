# Store Release Gate — DoodleRay store-win32

Go/no-go for a Partner Center submission. ANY unchecked box ⇒
**RC only, submission blocked.** This gate is additive to the main
docs/release-gate.md (reliability gate) — both must be green.

## 1. Build provenance

- [ ] Built by signed CI (`publish-downloads`, channel `store-win32`,
      `allow_unsigned=false`), not a local machine.
- [ ] Built from a tagged commit on the v6 release branch; commit recorded.
- [ ] `scripts/build-store.ps1` used (channel store-win32 baked; verified in
      bundle: no direct latest.json endpoint).

## 2. Signing gate

- [ ] `scripts/verify-signatures.ps1 -IncludeBuiltApp -InstallerPath <exe>`
      exit 0 on CI artifacts (DoodleRay.exe, DoodleRayService.exe,
      sing-box.exe, xray.exe, both wintun.dll, installer).
- [ ] Signature timestamps present (RFC3161).

## 3. Installer gate

- [ ] `verify-store-installer.ps1 -Force -UninstallAfter` exit 0 on
      Server 2022 + Win10 22H2 + Win11 23H2 + Win11 24H2 (see qa-matrix.md).
- [ ] Offline install proven (network disabled during install).
- [ ] Immutable versioned HTTPS URL live; SHA256 matches Partner Center entry.

## 4. Runtime reliability (inherited, must not regress)

- [ ] Full stand QA green on the store build (protected/degraded/limited
      honest states, auto-repair, fallback, support bundle, WinINet cleanup).
- [ ] Update-channel behavior per qa-matrix.md "Store-channel update behavior".
- [ ] No change to DoodleRayTunnelService semantics vs the direct build
      (byte-identical service binary).

## 5. Listing & policy

- [ ] Listing text taken from listing-draft.md; banned-phrase scan done.
- [ ] certification-notes.md pasted with a dedicated revocable reviewer
      activation code and sufficient device allowance.
- [ ] privacy-checklist.md fully checked; privacy policy URL live.
- [ ] Telemetry decision documented (disclosed or stripped).

## 6. Rollback story

- [ ] Previous store installer URL still live (immutable history).
- [ ] Support page documents manual downgrade path.
- [ ] In-app critical-update banner can point store users to a fixed version
      via `channels/store-win32/latest.json` without touching direct-channel users.

## Final verdict template

```
Store submission: GO / NO-GO
Blockers:
- ...
Evidence:
- CI run: ...
- QA matrix: ...
- Signatures: ...
```
