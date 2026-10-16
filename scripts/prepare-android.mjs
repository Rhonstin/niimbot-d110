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

export function configureCiSigning(gradle) {
  if (!gradle.includes('getByName("release")')) throw new Error("Generated Android release build type is missing.");
  const start = "// D110 CI signing";
  const end = "// /D110 CI signing";
  const block = `${start}
// Use optimized release code with a development certificate for CI installs.
// Distribution builds should supply a private release signing configuration.
android {
    buildTypes.getByName("release") {
        signingConfig = signingConfigs.getByName("debug")
        isDebuggable = false
        isJniDebuggable = false
    }
}
${end}`;
  const existing = new RegExp(`${start}[\\s\\S]*?${end}`);
  return existing.test(gradle) ? gradle.replace(existing, block) : `${gradle.trimEnd()}\n\n${block}\n`;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const manifest = new URL("../src-tauri/gen/android/app/src/main/AndroidManifest.xml", import.meta.url);
  await writeFile(manifest, configureManifest(await readFile(manifest, "utf8")));
  console.log("Configured Android Bluetooth permissions.");
  if (process.argv.includes("--ci-signing")) {
    const gradle = new URL("../src-tauri/gen/android/app/build.gradle.kts", import.meta.url);
    await writeFile(gradle, configureCiSigning(await readFile(gradle, "utf8")));
    console.log("Configured development signing for optimized release APKs.");
  }
}
