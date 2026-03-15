---
description: how to set up the project for local development (tauri dev)
---

# Dev Setup (Windows / macOS)

## Prerequisites
- Node.js 20+
- Rust (stable)
- Go 1.26+
- **Windows**: MSYS2 MinGW64 (for CGO)
- **macOS**: Xcode Command Line Tools (for CGO)

## Steps

// turbo-all

1. Install frontend deps:
```bash
npm ci
```

2. Build `libsingbox` from Go source (REQUIRED — not committed to git):

**Windows (PowerShell or MSYS2):**
```powershell
cd src-tauri/singbox-core
$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
$env:CGO_ENABLED = "1"
go build -tags "with_quic,with_utls,with_gvisor,with_dhcp,with_clash_api" -buildmode=c-shared -o ../libsingbox.dll .
cp ../libsingbox.dll ../  # copy to src-tauri/ root too
```

**macOS (bash):**
```bash
cd src-tauri/singbox-core
CGO_ENABLED=1 go build -tags "with_quic,with_utls,with_gvisor,with_dhcp,with_clash_api" \
  -buildmode=c-shared -o ../libsingbox.dylib .
```

3. Create stub files for Tauri resource bundling (if you don't have real sing-box binary):

**Windows:**
```bash
touch src-tauri/sing-box.exe
```

**macOS:**
```bash
touch src-tauri/sing-box && chmod +x src-tauri/sing-box
```

4. Create xray-core directory stub (if not present):
```bash
mkdir -p src-tauri/xray-core
touch src-tauri/xray-core/.gitkeep
```

5. Run the app:
```bash
npx tauri dev
```

## Build Tags Reference
| Tag | Purpose |
|-----|---------|
| `with_quic` | QUIC protocol support |
| `with_utls` | uTLS fingerprinting (chrome, firefox) |
| `with_gvisor` | gVisor network stack for TUN |
| `with_dhcp` | DHCP DNS server |
| `with_clash_api` | Clash API for traffic stats & connection logs |
