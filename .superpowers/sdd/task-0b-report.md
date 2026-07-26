# Task 0B report — canonical release metadata and local preflight

## Scope

- Worktree: `C:\Users\ilyae\.codex\worktrees\doodleray-repo-release-consolidation` (detached)
- No branch, tag, push, publish, CDN, GitHub, App Store, or production-state mutation performed.
- `backend_credits.md` was not read.

## Changes

- Added the canonical `release/release.json` for version `6.0.2`, macOS build `60017`, stable channel, and Windows/App Store targets.
- Added the Node-stdlib-only `scripts/release/check-release.mjs` preflight and focused Node test.
- The preflight enforces release schema, SemVer, mac build, stable channel, target booleans, all required version/build mirrors, App Store updater artifact disablement, and the existing direct updater endpoint/public key contract.
- `--published-version` is local-only and rejects a candidate that is not strictly newer.
- Corrected the existing package-lock and Xcode marketing-version drift to `6.0.2` while preserving macOS build `60017`.
- Made App Store build/package/upload scripts read release version and build from `release/release.json`, removed `6.0.0` artifact filenames, and run the preflight before work.
- Added `release:check` and the focused preflight test to `npm test`.

## TDD evidence

- RED: `node --test scripts/release/check-release.test.mjs` initially failed because the new checker module did not yet exist; the test cases specified lock/Xcode drift rejection and equal-published-version rejection.
- RED: after adding the checker but before metadata corrections, `node scripts/release/check-release.mjs --published-version 6.0.2` reported package-lock, XcodeGen, generated-pbxproj, and equal-published-version failures.
- RED: the App Store script test initially failed because all three scripts lacked `release/release.json` reads and still contained `6.0.0` filenames.
- GREEN: focused preflight suite passes 4/4.

## Verification

| Command | Result |
| --- | --- |
| `npm run release:check` | pass |
| `node scripts/release/check-release.mjs --published-version 6.0.1` | pass |
| `node scripts/release/check-release.mjs --published-version 6.0.2` | expected local failure: candidate must be strictly newer |
| `npm test` | pass; focused suite 4/4 plus existing Node tests |
| `npm run build` | pass; existing Vite chunk/dynamic-import warnings only |
| `cargo fmt --manifest-path .\src-tauri\Cargo.toml --all -- --check` | pass |
| `cargo test --manifest-path .\src-tauri\Cargo.toml --lib` | pass; 107 passed, 3 ignored |
| `cargo test --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service` | pass; 4 passed |
| `cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRay` | pass |
| `cargo check --manifest-path .\src-tauri\Cargo.toml --bin DoodleRayService --features windows-service` | pass |
| `git diff --check` | pass |

## Self-review and concerns

- Identifiers, Team/App Group/extension IDs, updater public key/direct endpoint, API/VPN behavior, installer identity, and publication flows are unchanged.
- No macOS signing, packaging, upload, or live VPN run was performed in this Windows worktree.
- The existing Vite chunk/dynamic-import warnings remain outside this task's scope.
