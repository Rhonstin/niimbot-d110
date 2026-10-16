import test from "node:test";
import assert from "node:assert/strict";
import { configureManifest } from "./prepare-android.mjs";

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
