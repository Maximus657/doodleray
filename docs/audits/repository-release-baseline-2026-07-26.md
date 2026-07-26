# Repository and release baseline — 2026-07-26

This is a read-only snapshot taken before repository cleanup. No branch, tag,
release, CDN object, App Store build, or GitHub setting was changed while
collecting it.

## Repository state

- Repository: `Maximus657/doodleray`
- Default branch: `main`
- Baseline `origin/main`: `7d16836fc8d2f6583452f21378ef95ad613b33c5`
- Open pull requests: `0`
- Remote branches: `12` total (`main` plus 11 non-main branches)
- Git tags fetched: `103`
- GitHub Releases: `123` (`v2.0.0` through `v6.0.2`)
- Repository rulesets returned by GitHub: none
- Branch protection endpoint for `main`: not configured/available (`404`)
- Automatic deletion of merged head branches: disabled

The local implementation worktree is detached and is not represented by a
GitHub branch.

## Remote branch preservation inventory

`Ahead` is the number of commits reachable from the branch and not from the
baseline `origin/main`. `Patch-unique` is the corresponding `git cherry`
count; it is evidence about patch identity, not a claim that later code lacks
equivalent behavior.

| Remote branch | Tip | Ahead | Behind | Patch-unique | Preservation decision before deletion |
| --- | --- | ---: | ---: | ---: | --- |
| `claude/windows-6.0.1-rc-hardening` | `c65981e50a7f40539cea6961a5aa20b1dccccd02` | 0 | 7 | 0 | Fully reachable from `main` |
| `codex/release-5.2.1` | `d769e1a049306e730221a077d9ffaf6aa308a52f` | 0 | 108 | 0 | Fully reachable from `main`; release history exists |
| `codex/release-5.9.1` | `b988abc44051234ba3111b37eba7bb2e66804ca2` | 17 | 98 | 17 | Preserved by annotated tag `v5.9.1` at this commit |
| `codex/v6-macos-app-store` | `23bf79d7e851e4453457cf155b7d0a1030b54ed7` | 0 | 40 | 0 | Fully reachable from `main` |
| `codex/v6-store-redesign` | `f9a574c94fedea952a15b567c569a3d6c4e9ab91` | 9 | 80 | 9 | Create an archive tag before deletion |
| `codex/v6-windows-6.0.1` | `fd06c672a06d67f0928b8429fdbe958699f6c6a8` | 5 | 31 | 5 | Create an archive tag before deletion |
| `codex/v6-windows-production` | `aebf6321b7cc5268cfa9a767c7a2279bbee42e1b` | 0 | 37 | 0 | Fully reachable from `main` |
| `codex/windows-one-click-vpn` | `42d3c34195f377f3352c2a6edc9334158ebcb62e` | 4 | 98 | 4 | Preserved by annotated tag `v5.9.0` at this commit |
| `develop` | `81b7cc6881b499629c4f58509063cde8ca123800` | 0 | 80 | 0 | Fully reachable from `main` |
| `fix/keyless-migration-hardening` | `ca14e27d71e7ba2f0df26e02ab9ef540e21557bc` | 1 | 26 | 1 | Preserve with an archive tag after the reviewed semantic transfer |
| `production` | `81b7cc6881b499629c4f58509063cde8ca123800` | 0 | 80 | 0 | Fully reachable from `main` |

No remote branch is safe to delete merely because its PR is closed. The three
untagged unique tips above must be tagged first, and the migration branch must
remain until its independently reviewed replacement reaches `main`.

## Current release and updater baseline

- Latest GitHub Release: `v6.0.2`, published 2026-07-25, target commit
  `1b8f33a495260447ca30555d0750de7a435576f4`.
- The `v6.0.2` commit is an ancestor of the current `main`.
- Live direct updater manifest:
  `https://doodleray.clickflare.click/channels/direct/latest.json`
- Live manifest version: `6.0.2`; HTTP status: `200`.
- Live manifest SHA-256 at audit time:
  `d99619b01151e1a73128c4236c5493c2a357997b94b40f88c97160f8afd3f3bb`.
- GitHub Release `latest.json` SHA-256:
  `9b879d858ac48b5f9cfa074ce7d9458aa8d46f5287606286c8c9df864ca4dbaa`.

The CDN and GitHub manifests intentionally reference their own hosts, but the
corresponding signatures and artifact bytes also differ. This violates the
target invariant that QA, GitHub, and CDN publish one immutable Windows build.

| Artifact | CDN SHA-256 | GitHub Release SHA-256 | Exact bytes |
| --- | --- | --- | --- |
| `DoodleRay_6.0.2_x64-setup.nsis.zip` | `608c06edcb5c3a6b78e995434b6bf84aefce4d2868d0e540cf25ef3db116b3af` | `598761ff9b1987ae6e4b21c029aef341fa9d62d1e2dcdfa0069fe6c9037fc130` | No |
| `DoodleRay_6.0.2_x64-setup.exe` | `3dfc46fba8c776b9dab44ac29e721ce3057cdaf5def549c02a37ffd0e08afe4d` | `c791cdce4a1a3db9cfcfcd6228f1ae0a60636e0bb2fcda7b651a21d039b8e040` | No |
| `DoodleRay_6.0.2_x64-setup.nsis.zip.sig` | `4a4e403ac0746bb50f21ffc8d4c1561b0a005017b93131e30873e8bc59a175b6` | `39ab3a39cff646c481669dc9b8c55b6b5a6e59c2c50cc8b128f40de4dc144702` | No |
| `DoodleRay_6.0.2_x64-setup.exe.sig` | `38c73db5c9430483661317234ca69438dc0148121dcde84a1f341121b2179b86` | `ff145be6c35067e46263aefe8586b367409b34ac9181e6916f255455bead972b` | No |

The live CDN manifest and the immutable `6.0.2` directory must not be rewritten
during the refactor. The difference is baseline evidence to eliminate in the
next version by building Windows once and promoting exactly those bytes.

## Active GitHub Actions baseline

Five workflows are active:

- `publish-downloads`
- `Build & Release`
- `runtime-updates`
- `store-win32`
- `Windows v6 RC gate`

The tag-triggered `Build & Release` run for `v6.0.2` failed. A later
`publish-downloads` dispatch from `main` succeeded and produced the current CDN
state, which is consistent with the byte split recorded above. Consolidation
must be rehearsed before any old workflow is disabled or removed.

## Safety verdict

The snapshot is complete enough to protect the current updater and decide
which branch tips need archival tags. It is not production-readiness evidence:
the in-place upgrade/clean-OS matrix and macOS TestFlight checks are still
missing.

**RC only, production blocked.**
