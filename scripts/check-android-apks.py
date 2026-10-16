"""Reject oversized APKs, mixed ABIs, and unstripped Rust debug libraries."""

from pathlib import Path
import os
import re
import subprocess
import tempfile
import zipfile


def inspect_apk(apk, abi, maximum=25 * 1024 * 1024):
    size = apk.stat().st_size
    if size > maximum:
        raise ValueError(f"{apk.name}: {size:,} bytes exceeds the {maximum:,}-byte APK limit")
    with zipfile.ZipFile(apk) as archive:
        architectures = {p.split("/")[1] for p in archive.namelist() if p.startswith("lib/") and p.endswith(".so")}
        if architectures != {abi}:
            raise ValueError(f"{apk.name}: expected only {abi}, found {architectures}")
        library = f"lib/{abi}/libniimbot_d110.so"
        info = archive.getinfo(library)
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "libniimbot_d110.so"
            binary.write_bytes(archive.read(library))
            sections = subprocess.check_output(["readelf", "--sections", "--wide", binary], text=True)
            if re.search(r"\.(?:z?debug_)\w+", sections):
                raise ValueError(f"{apk.name}: Rust debug symbols are still included")
    return size, info.file_size


def main():
    root = Path("src-tauri/gen/android/app/build/outputs/apk")
    rows = ["| APK | Size | Native library |", "| --- | ---: | ---: |"]
    for flavor, abi in [("arm64", "arm64-v8a"), ("x86_64", "x86_64")]:
        matches = list((root / flavor / "release").glob("*.apk"))
        if len(matches) != 1:
            raise ValueError(f"Expected one {flavor} release APK, found {len(matches)}")
        apk = matches[0]
        size, native_size = inspect_apk(apk, abi)
        print(f"{apk.name}: {size:,} bytes; native library: {native_size:,} bytes; stripped, single ABI")
        rows.append(f"| {apk.name} | {size / 1_000_000:.2f} MB | {native_size / 1_000_000:.2f} MB |")
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a") as stream:
            stream.write("\n".join(rows) + "\n")


if __name__ == "__main__":
    main()
