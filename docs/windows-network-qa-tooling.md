# Windows Network QA Tooling

This page lists the practical tools to use for DoodleRay PC Windows networking
QA. Prefer tools that produce evidence for DNS, proxy, route, listener, packet,
or HTTP/TLS behavior. Do not install random community skills into release
workflows without source review and pinned versions.

All subscription-dependent checks use the canonical DoodleVPN test subscription
described in `docs/qa-test-subscription.md`. Keep the raw URL in
`secrets/doodlevpn-test-subscription-url.txt` only.

## Trusted Baseline Tools

- Microsoft `netsh trace`
  - Use for short Windows ETW/network captures around subscription fetch,
    updater, proxy mode, and protected/TUN connect failures.
  - Keep raw ETL files on the QA server only unless explicitly redacted.
  - Source: https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netsh-trace

- Microsoft `pktmon`
  - Use for packet counters, packet capture, drop detection, and virtualization
    visibility when TUN, Wintun, routes, or adapter behavior is suspected.
  - Source: https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/pktmon

- NSA Cyber `HTTP-Connectivity-Tester`
  - Use for repeatable HTTP/HTTPS reachability checks with DNS, status, and TLS
    signal collection.
  - Download as a zip on clean servers because Git may not be installed.
  - Source: https://github.com/nsacyber/HTTP-Connectivity-Tester

- `curl.exe`
  - Use as a small independent HTTP client for direct/proxy probes, status code,
    response size, and timeout checks.
  - Source: https://curl.se/docs/manpage.html

- `mitmproxy` / `mitmdump`
  - Use for explicit HTTP proxy lab tests and protocol inspection when a client
    can be pointed at a proxy. Do not use as a production dependency.
  - Source: https://github.com/mitmproxy/mitmproxy

- Wintun references
  - Use for understanding adapter behavior and Windows TUN lab reproductions.
  - Source: https://github.com/WireGuard/wintun

## Agent Skill Sources

No high-confidence ready-made "Windows VPN QA" Codex skill was found in the
official curated list. Use these only for discovery and workflow ideas:

- Official Codex skills discussion/docs:
  https://community.openai.com/t/skills-for-codex-experimental-support-starting-today/1369367
- Awesome Codex CLI:
  https://github.com/RoggeOhta/awesome-codex-cli
- Awesome Agent Skills:
  https://github.com/VoltAgent/awesome-agent-skills
- QA skill pack reference:
  https://github.com/neonwatty/qa-skills
- Microsoft skills:
  https://github.com/microsoft/skills

## 2026-07-01 Subscription Fetch Evidence

On the Play2Go Windows QA stand:

- The canonical DoodleVPN test subscription from the ignored
  `secrets/doodlevpn-test-subscription-url.txt` file was used; see
  `docs/qa-test-subscription.md`.
- `nsacyber/HTTP-Connectivity-Tester` was downloaded as a zip and used for
  redacted HTTPS checks.
- WinINet was clean after the test: `ProxyEnable=0`, no proxy server, no proxy
  override.
- WinHTTP was direct.
- The subscription host (kept only in the ignored secret file) resolved
  successfully.
- The real subscription endpoint returned HTTP `200` with non-empty content via
  both `Invoke-WebRequest` and `curl.exe`; the subscription URL and content were
  not written to committed logs.
- The release Rust harness test
  `tests::windows_subscription_fetch_uses_system_proxy_fallback` passed.
- A short `netsh trace scenario=InternetClient capture=yes` was captured around
  the subscription fetch. Raw ETL remains on the QA server only.
