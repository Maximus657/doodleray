# Store QA Matrix — DoodleRay store-win32 flavor

All rows must pass on the Play2Go stand or clean VMs (never the dev PC),
with the **signed** store-flavor installer from `scripts/build-store.ps1`.
Evidence goes to docs/windows-tun-release-qa-report.md style redacted logs.

## OS matrix

| OS | Install /S | Connect (Whole computer) | Browsers mode | Manual mode | Uninstall clean |
|---|---|---|---|---|---|
| Windows Server 2022 (Play2Go) | ☐ | ☐ | ☐ | ☐ | ☐ |
| Windows 10 22H2 clean VM | ☐ | ☐ | ☐ | ☐ | ☐ |
| Windows 11 23H2 clean VM | ☐ | ☐ | ☐ | ☐ | ☐ |
| Windows 11 24H2 clean VM | ☐ | ☐ | ☐ | ☐ | ☐ |

## Per-install assertions (scripted)

- `scripts/verify-store-installer.ps1 -Force -UninstallAfter` → exit 0
  (signature, /S, Apps&Features metadata, Start Menu, service, WebView2,
  installed-PE signatures, clean uninstall).
- `scripts/verify-signatures.ps1 -IncludeBuiltApp -InstallerPath <exe>` → exit 0.
- Offline check: disable network → run installer /S → app launches (WebView2
  offline installer proves itself here).

## Protected-mode assertions (reuse existing QA)

Run the standard flow from docs/windows-pc-qa-play2go.md /
scripts/windows-qa/Invoke-DoodleRayFullStandQa.ps1 against the store build:

- verdict `protected` or honest `protected_degraded`;
- structured SOCKS/HTTP/API ports; listeners accept;
- TUN adapter "DoodleRay" alias/ifIndex present; IPv4 route coverage;
- DNS path expected, no known leak;
- HTTPS/WS/SSE/UDP-STUN probes pass or honestly degraded;
- Telegram/Discord/OpenAI/Claude probes checked;
- RU split-direct behavior per product rules; endpoint bypass route verified;
- fallback Protected→Browsers shows LIMITED (never fake green);
- no orphan xray/sing-box/statsquery after disconnect/failure.

## Store-channel update behavior

- [ ] Store build shows update banner when `channels/store-win32/latest.json` advertises
      a newer version.
- [ ] Default policy: pressing the banner button opens the Store/support page
      (no in-app download); phase returns to `available`, no stuck spinner.
- [ ] With `-EnableSelfUpdate` build: in-app update downloads, minisign
      signature verified by updater, PrepareForUpdate runs, app relaunches.
- [ ] Store build never contacts the direct-channel `latest.json`.
      (Verify via logs/proxy: only `channels/store-win32/latest.json` endpoint.)

## Upgrade rows

| From | To | Expectation |
|---|---|---|
| Direct 5.4.x installed | Store installer /S over it | Single instance, service upgraded, settings preserved |
| Store build N | Store build N+1 (per policy) | User-initiated path works; no silent downgrade of protection |

## Cleanup contract (every row ends with)

`scripts/verify-clean-uninstall.ps1` → exit 0: no service, no orphan engines,
WinINet clean, no DoodleRay NRPT/routes/adapter, no marker, no scheduled
tasks, no Apps&Features entry. Never leave the stand dirty.
