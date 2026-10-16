import test from "node:test";
import assert from "node:assert/strict";
import { configureCiSigning, configureManifest } from "./prepare-android.mjs";

test("Android manifest keeps app settings and configures required BLE permissions", () => {
  const input = '<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application android:label="D110 Studio" /></manifest>';
  const output = configureManifest(input);
  assert.ok(output.includes('<application android:label="D110 Studio" />'));
  assert.match(output, /BLUETOOTH_SCAN" android:usesPermissionFlags="neverForLocation"/);
  assert.match(output, /ACCESS_FINE_LOCATION" android:maxSdkVersion="30"/);
  assert.match(output, /android.hardware.bluetooth_le" android:required="true"/);
  assert.equal(configureManifest(output), output);
});

test("invalid generated manifest fails explicitly", () => {
  assert.throws(() => configureManifest(""), /invalid/);
});

test("CI release signing preserves optimization and JNI rules and is idempotent", () => {
  const input = 'android { buildTypes { getByName("release") { optimization { enable = true }; proguardFiles("proguard-rules.pro") } } }';
  const output = configureCiSigning(input);
  assert.ok(output.startsWith(input));
  assert.match(output, /signingConfig = signingConfigs.getByName\("debug"\)/);
  assert.match(output, /isDebuggable = false/);
  assert.match(output, /isJniDebuggable = false/);
  assert.equal(configureCiSigning(output), output);
  assert.throws(() => configureCiSigning(""), /release build type/);
});
