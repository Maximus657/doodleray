# Public Release Notes

This directory contains user-facing release notes for the download page.

Create one file per public version:

```text
docs/release-notes/5.9.0.md
docs/release-notes/6.0.0.md
```

Required format:

```markdown
# DoodleRay 6.0.0

Дата: 8 июля 2026

Коротко: one simple sentence explaining why this update matters.

- One short change that a normal user can understand.
- Another short change that explains the benefit, not only the internals.
```

Rules:

- Write in simple Russian for normal users.
- Explain the benefit: “подключение восстанавливается без перезагрузки” is
  better than “fixed adapter generation mismatch”.
- Avoid raw subscription links, endpoint IPs, keys, UUIDs, and internal support
  logs.
- Keep each bullet short enough to fit on the download page.

`scripts/release/Publish-DoodleRayDownloads.ps1` reads this file, creates
`release-notes.json`, updates `history.json`, and renders the public
“История версий” block.
