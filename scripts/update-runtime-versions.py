#!/usr/bin/env python3

import argparse
import copy
import hashlib
import json
import os
import re
import urllib.parse
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
USER_AGENT = "Mozilla/5.0 DoodleRay-runtime-updater/1.0"
SEMVER = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")


def fetch(url, *, github=False):
    headers = {"User-Agent": USER_AGENT}
    if github:
        headers["Accept"] = "application/vnd.github+json"
        if token := os.environ.get("GITHUB_TOKEN"):
            headers["Authorization"] = f"Bearer {token}"
    with urllib.request.urlopen(
        urllib.request.Request(url, headers=headers), timeout=60
    ) as response:
        return response.read()


def fetch_json(url, *, github=False):
    return json.loads(fetch(url, github=github))


def github_json(path):
    return fetch_json(f"https://api.github.com{path}", github=True)


def release(repo, tag):
    return github_json(
        f"/repos/{repo}/releases/tags/{urllib.parse.quote(tag, safe='')}"
    )


def latest_release(repo, *, stable_only):
    candidates = []
    for item in github_json(f"/repos/{repo}/releases?per_page=100"):
        match = SEMVER.fullmatch(item["tag_name"])
        if not item["draft"] and match and (not stable_only or not item["prerelease"]):
            candidates.append((tuple(map(int, match.groups())), item))
    if not candidates:
        raise RuntimeError(f"No usable official release found for {repo}")
    return max(candidates, key=lambda candidate: candidate[0])[1]


def assets_from_release(item, names):
    available = {asset["name"]: asset for asset in item["assets"]}
    result = {}
    for platform, name in names.items():
        asset = available.get(name)
        digest = asset and asset.get("digest")
        if not digest or not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise RuntimeError(
                f"Official SHA256 metadata is missing for {item['tag_name']}/{name}"
            )
        result[platform] = {"name": name, "sha256": digest.removeprefix("sha256:")}
    return result


def assert_equal(actual, expected, label):
    if actual != expected:
        raise RuntimeError(f"{label}: expected {expected}, got {actual}")


def gomobile_sum(version):
    lookup = fetch(
        "https://sum.golang.org/lookup/golang.org/x/mobile@"
        + urllib.parse.quote(version, safe="")
    ).decode()
    prefix = f"golang.org/x/mobile {version} "
    for line in lookup.splitlines():
        if line.startswith(prefix):
            return line.split()[2]
    raise RuntimeError(f"Go checksum database has no sum for gomobile {version}")


def verify(data):
    xray = data["xray"]
    official_xray = assets_from_release(
        release("XTLS/Xray-core", xray["version"]),
        {platform: asset["name"] for platform, asset in xray["assets"].items()},
    )
    assert_equal(official_xray, xray["assets"], "Xray assets")

    libxray = data["libxray"]
    official_commit = github_json(
        f"/repos/XTLS/libXray/commits/{urllib.parse.quote(libxray['tag'], safe='')}"
    )["sha"]
    assert_equal(official_commit, libxray["commit"], "LibXray tag commit")

    sing_box = data["sing_box"]
    official_sing_box = assets_from_release(
        release("SagerNet/sing-box", f"v{sing_box['version']}"),
        {platform: asset["name"] for platform, asset in sing_box["assets"].items()},
    )
    assert_equal(official_sing_box, sing_box["assets"], "sing-box assets")

    gomobile = data["gomobile"]
    assert_equal(gomobile_sum(gomobile["version"]), gomobile["sum"], "gomobile sum")

    wintun = data["wintun"]
    wintun_bytes = fetch(f"https://www.wintun.net/builds/{wintun['asset']}")
    assert_equal(
        hashlib.sha256(wintun_bytes).hexdigest(), wintun["sha256"], "Wintun archive"
    )


def updated(data):
    result = copy.deepcopy(data)

    xray_release = latest_release("XTLS/Xray-core", stable_only=True)
    xray_version = xray_release["tag_name"]
    result["xray"] = {
        "version": xray_version,
        "assets": assets_from_release(
            xray_release,
            {
                "windows_amd64": "Xray-windows-64.zip",
                "darwin_arm64": "Xray-macos-arm64-v8a.zip",
                "darwin_amd64": "Xray-macos-64.zip",
            },
        ),
    }

    libxray_release = latest_release("XTLS/libXray", stable_only=True)
    libxray_tag = libxray_release["tag_name"]
    result["libxray"] = {
        "tag": libxray_tag,
        "commit": github_json(
            f"/repos/XTLS/libXray/commits/{urllib.parse.quote(libxray_tag, safe='')}"
        )["sha"],
    }

    sing_box_release = latest_release("SagerNet/sing-box", stable_only=True)
    sing_box_version = sing_box_release["tag_name"].removeprefix("v")
    result["sing_box"] = {
        "version": sing_box_version,
        "assets": assets_from_release(
            sing_box_release,
            {
                "windows_amd64": f"sing-box-{sing_box_version}-windows-amd64.zip",
                "darwin_arm64": f"sing-box-{sing_box_version}-darwin-arm64.tar.gz",
                "darwin_amd64": f"sing-box-{sing_box_version}-darwin-amd64.tar.gz",
            },
        ),
    }

    gomobile_info = fetch_json(
        "https://proxy.golang.org/golang.org/x/mobile/@latest"
    )
    gomobile_version = gomobile_info["Version"]
    result["gomobile"] = {
        "version": gomobile_version,
        "sum": gomobile_sum(gomobile_version),
    }

    # Wintun publishes binaries without independent checksum metadata. Keep the
    # reviewed version pinned; verify its locked hash instead of guessing updates.
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest", type=Path, default=ROOT / "runtime-versions.json"
    )
    parser.add_argument(
        "--check", action="store_true", help="verify current pins without editing"
    )
    args = parser.parse_args()

    data = json.loads(args.manifest.read_text())
    if args.check:
        verify(data)
        print("Runtime pins match official metadata and locked checksums.")
        return

    new_data = updated(data)
    if new_data == data:
        print("Runtime pins are current.")
        return
    args.manifest.write_text(json.dumps(new_data, indent=2) + "\n")
    print(f"Updated {args.manifest}")


if __name__ == "__main__":
    main()
