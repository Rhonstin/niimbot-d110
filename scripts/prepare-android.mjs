import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

// tauri android init generates the native project from the pinned CLI version.
// The BLE plugin merges its own permissions. Restrict legacy location access
// to Android <= 11; Android 12+ uses Nearby devices, without deriving location.
export function configureManifest(xml) {
  const permissions = [
    '<uses-feature android:name="android.hardware.bluetooth_le" android:required="true" />',
    '<uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30" />',
    '<uses-permission android:name="android.permission.BLUETOOTH_ADMIN" android:maxSdkVersion="30" />',
    '<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" android:maxSdkVersion="30" />',
    '<uses-permission android:name="android.permission.ACCESS_COARSE_LOCATION" android:maxSdkVersion="30" />',
    '<uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />',
    '<uses-permission android:name="android.permission.BLUETOOTH_SCAN" android:usesPermissionFlags="neverForLocation" />',
  ];
  const start = "<!-- D110 Bluetooth permissions -->";
  const end = "<!-- /D110 Bluetooth permissions -->";
  const block = `${start}\n    ${permissions.join("\n    ")}\n    ${end}`;
  if (!xml.includes("</manifest>")) throw new Error("Generated Android manifest is invalid.");
  const existing = new RegExp(`${start}[\\s\\S]*?${end}`);
  return existing.test(xml) ? xml.replace(existing, block) : xml.replace("</manifest>", `    ${block}\n</manifest>`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const manifest = new URL("../src-tauri/gen/android/app/src/main/AndroidManifest.xml", import.meta.url);
  await writeFile(manifest, configureManifest(await readFile(manifest, "utf8")));
  console.log("Configured Android Bluetooth permissions.");
}
