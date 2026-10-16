"""Publish installers from an explicitly selected, successful CI run."""

import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


DESKTOP = {
    "x86_64-unknown-linux-gnu": ("linux-x64", (".deb", ".rpm", ".AppImage")),
    "aarch64-unknown-linux-gnu": ("linux-arm64", (".deb", ".rpm", ".AppImage")),
    "x86_64-pc-windows-msvc": ("windows-x64", (".exe", ".msi")),
    "aarch64-apple-darwin": ("macos-arm64", (".dmg",)),
    "x86_64-apple-darwin": ("macos-x64", (".dmg",)),
}
ARTIFACTS = {f"d110-studio-{target}" for target in DESKTOP} | {"d110-studio-android"}


def gh(*args):
    return subprocess.check_output(["gh", *map(str, args)], text=True).strip()


def api(repo, path):
    return json.loads(gh("api", f"repos/{repo}/{path}"))


def validate_run(request, run):
    if not re.fullmatch(r"v\d+\.\d+\.\d+", request["tag"]):
        raise ValueError("Release tag must be vMAJOR.MINOR.PATCH")
    if not re.fullmatch(r"[0-9a-f]{40}", request["commit"]):
        raise ValueError("Release commit must be a full SHA")
    if run["status"] != "completed" or run["conclusion"] != "success":
        raise ValueError("Release requires a successful, completed CI run")
    if run["head_sha"] != request["commit"] or run["id"] != request["run_id"]:
        raise ValueError("CI run does not match the requested source commit")
    if run["path"] != ".github/workflows/ci.yml":
        raise ValueError("Installers must come from the build-and-test workflow")


def collect_assets(root, version):
    assets = []
    for target, (platform, extensions) in DESKTOP.items():
        directory = root / f"d110-studio-{target}"
        for extension in extensions:
            matches = sorted(p for p in directory.rglob(f"*{extension}") if p.is_file())
            if len(matches) != 1 or matches[0].stat().st_size == 0:
                raise ValueError(f"Expected one nonempty {platform} {extension} installer")
            assets.append((matches[0], f"d110-studio_{version}_{platform}{extension}"))
    directory = root / "d110-studio-android"
    for abi, platform in [("arm64", "arm64"), ("x86_64", "x64")]:
        matches = sorted(p for p in directory.rglob(f"app-{abi}-release.apk") if p.is_file())
        if len(matches) != 1 or matches[0].stat().st_size == 0:
            raise ValueError(f"Expected one nonempty Android {abi} APK")
        assets.append((matches[0], f"d110-studio_{version}_android-{platform}.apk"))
    return assets


def main():
    repo = os.environ["GITHUB_REPOSITORY"]
    request = json.loads(Path(".github/release.json").read_text())
    run = api(repo, f"actions/runs/{request['run_id']}")
    validate_run(request, run)
    tag = request["tag"]
    version = tag[1:]
    notes = Path(f".github/release-notes/{tag}.md")
    if not notes.read_text().strip():
        raise ValueError("Release notes are required")
    for name in ["package.json", "src-tauri/tauri.conf.json"]:
        source = api(repo, f"contents/{name}?ref={request['commit']}")
        if json.loads(base64.b64decode(source["content"]))["version"] != version:
            raise ValueError(f"{name} at the CI commit does not match {tag}")
    artifacts = api(repo, f"actions/runs/{request['run_id']}/artifacts?per_page=100")["artifacts"]
    available = {a["name"] for a in artifacts if not a["expired"]}
    if not ARTIFACTS <= available:
        raise ValueError(f"Missing or expired CI artifacts: {sorted(ARTIFACTS - available)}")
    # An existing tag must resolve to the tested commit. A missing tag is created
    # by gh release create using the explicit --target commit below.
    existing = subprocess.run(
        ["gh", "api", f"repos/{repo}/git/ref/tags/{tag}"],
        text=True, capture_output=True,
    )
    if existing.returncode == 0:
        if api(repo, f"commits/{tag}")["sha"] != request["commit"]:
            raise ValueError("Existing release tag points to another commit")
    elif "404" not in existing.stderr:
        raise RuntimeError(existing.stderr)
    with tempfile.TemporaryDirectory(prefix="d110-release-") as directory:
        root = Path(directory)
        downloads = root / "artifacts"
        downloads.mkdir()
        gh("run", "download", request["run_id"], "--repo", repo, "--dir", downloads)
        output = root / "assets"
        output.mkdir()
        for source, name in collect_assets(downloads, version):
            shutil.copyfile(source, output / name)
        installers = sorted(output.iterdir())
        checksums = []
        for path in installers:
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            checksums.append(f"{digest}  {path.name}\n")
        (output / "SHA256SUMS.txt").write_text("".join(checksums))
        # gh creates a draft, uploads every asset, then publishes the release.
        url = gh(
            "release", "create", tag, *sorted(output.iterdir()), "--repo", repo,
            "--target", request["commit"], "--title", f"D110 Studio {tag}",
            "--notes-file", notes, "--latest",
        )
        print(url)
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a") as stream:
                stream.write(f"Published [{tag}]({url}) with 12 installers and SHA-256 checksums.\n")


if __name__ == "__main__":
    main()
