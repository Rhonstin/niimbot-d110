# D110 Studio

A local **Tauri 2** label editor for the **NIIMBOT D110**, with Bluetooth LE
printing on Linux, Windows, macOS, and Android.

English is the default interface language. Choose **English / Українська** in
the top bar; the selection is remembered. Changing the interface language
preserves your label text, images, and editor settings.

## Features

- Discover D110 printers, connect, identify the printer model, and disconnect.
- Text, system fonts, alignment, automatic text fitting, and image import.
- Monochrome preview and PNG export through the system file picker.
- Label dimensions, density 1–5, up to 20 copies, rotation, and feed offset.
- Progress, cancellation, diagnostics, and local editor persistence.
- Native BlueZ transport on Linux; `tauri-plugin-blec` on Windows, macOS,
  and Android. The same Rust protocol implementation drives every platform.

## Development

Install Node.js 22+, Rust stable, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.
Linux also needs BlueZ and the D-Bus development package. On Fedora:

```bash
sudo dnf install gcc gcc-c++ pkgconf-pkg-config gtk3-devel webkit2gtk4.1-devel dbus-devel bluez
```

```bash
git clone https://github.com/Rhonstin/niimbot-d110.git
cd niimbot-d110
npm ci
npm run tauri dev
```

Turn on the printer and Bluetooth, and close NIIMBOT on your phone.
Choose **Find printer → Connect**, then select the actual loaded label size.
**Test label** fills the editor; **Print** submits the displayed artwork.

`npm run dev` opens the editor in a browser. Native Bluetooth is available
inside the Tauri app. Browser mode still supports editing and PNG export.

On macOS the system requests Bluetooth permission. Android requires version
8.0 (API 26) or later. On Android allow **Nearby
devices**; older Android versions also require location permission for BLE
discovery. The app does not use Bluetooth scanning to derive your location.

## Continuous integration and downloads

[Download the latest public release](https://github.com/Rhonstin/niimbot-d110/releases/latest).
Releases include installers for every platform and `SHA256SUMS.txt`.

[Build and test](https://github.com/Rhonstin/niimbot-d110/actions/workflows/ci.yml)
runs on pushes to `main`, version tags (`v*`), pull requests, and manual runs.
Download installers from the workflow run's **Artifacts** section:

| Platform | Architectures | Output |
| --- | --- | --- |
| Linux | x64, ARM64 | `.deb`, `.rpm`, `.AppImage` |
| Windows | x64 | NSIS `.exe`, WiX `.msi` |
| macOS | Apple Silicon, Intel | `.dmg` |
| Android | ARM64, x64 emulator | Optimized release `.apk`, one per ABI |

The checks job builds the frontend, runs the protocol, bitmap, localization,
and Android manifest tests, then runs Rust formatting and Clippy. Platform
builds start only after those checks pass. Cargo/npm lockfiles are committed;
build artifacts are retained for 14 days. Builds do not require printer hardware.

Desktop installers are not signed/notarized with a distribution certificate.
Android APKs use release optimization, R8, stripped native libraries, and the
runner's development signing key. CI checks each APK is under 25 MiB and contains
only its intended ABI, without native debug symbols. They are installable test
builds; each CI run creates a new development certificate.
Store publishing and production signing are separate from these CI builds.

To publish a release, add `.github/release-notes/vX.Y.Z.md` and update
`.github/release.json` with its tag, full source commit, and successful CI run ID.
Pushing that request to `main` starts the publish workflow, which verifies the
source version and all platform artifacts before creating the release. The tag
points to the tested source commit. Existing releases are never overwritten.

## Local builds

```bash
# Build the installers supported by the current desktop OS.
npm run tauri build

# Build only a Fedora RPM.
npm run tauri build -- --bundles rpm
```

Desktop binaries and bundles are under `target/release/` and
`target/release/bundle/` (or `target/<target-triple>/release/` when specifying
`--target`). Debug builds use `--debug` and the corresponding `debug/` directory.

For Android, install JDK 17, the Android SDK/NDK, and the Rust targets first:

```bash
npm run tauri android init -- --ci
node scripts/prepare-android.mjs --ci-signing
npm run tauri icon -- src-tauri/icons/icon.svg
CARGO_PROFILE_RELEASE_OPT_LEVEL=s npm run tauri android build -- --ci --apk --split-per-abi --target aarch64 --target x86_64
```

The native Android project is generated from the pinned Tauri CLI. The setup
script adds BLE permissions and limits legacy location permissions to Android
11 and earlier. APKs are written under
`src-tauri/gen/android/app/build/outputs/apk/`.

## Verification

```bash
npm run check
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`cargo test -p niimbot-core` exercises the transport-independent print protocol
without desktop libraries or printer hardware. Actual Bluetooth connections,
paper registration, and firmware compatibility require a physical D110.

## Printer protocol

Supported model: D110 (`2304 / 0x0900`). The casing marking
`R-R-WUj-D110A` is not the Bluetooth identity. D110_M (`2320`) uses a different
sequence and is explicitly rejected in this version.

The printhead is 96 pixels wide at 203 dpi, about 12 mm of printable width.
A 15 mm label retains white side margins; a 30 mm feed length is 240 rows.
The landscape preview rotates into a `96 × feed length` bitmap for transmission.
Pixels are MSB-first and repeated rows are grouped in runs of at most 200.

Each copy is a separate job: D110 firmware can accept a multi-copy setting
while printing only its first page. The B1-family sequence uses a 7-byte
PrintStart, PageStart, a 6-byte SetPageSize, bitmap rows, PageEnd, a completed
page counter, and PrintEnd. Unacknowledged BLE writes are paced at 10 ms and
split according to MTU (20-byte fallback).

Success requires PageEnd acknowledgment, a completed page count of at least 1,
and PrintEnd acknowledgment. Failures stop remaining copies, close the job,
and require reconnection. Uncertain prints are never retried automatically;
check the physical label before repeating a failed/cancelled job.

## Source layout

- `crates/niimbot-core`: framing, decoder, row encoding, print lifecycle.
- `src-tauri/src/printer.rs`: shared models, identity checks, handshake, logging.
- `src-tauri/src/bluez.rs`: Linux BlueZ transport.
- `src-tauri/src/ble.rs`: Windows/macOS/Android BLE transport.
- `src-tauri/src/lib.rs`: Tauri commands, worker state, cancellation, file export.
- `src/bitmap.ts`: monochrome conversion and printhead rotation.
- `src/i18n.ts`: English/Ukrainian UI messages and language preference.
- `.github/workflows/ci.yml`: checks and installer/APK build matrix.

Protocol attribution: [third_party/NOTICES.md](third_party/NOTICES.md).
