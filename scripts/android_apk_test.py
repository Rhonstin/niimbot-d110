import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


spec = importlib.util.spec_from_file_location("apk_checks", Path(__file__).with_name("check-android-apks.py"))
apk_checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apk_checks)


@unittest.skipUnless(all(shutil.which(tool) for tool in ["cc", "strip", "readelf"]), "Requires native ELF tools")
class AndroidApkTest(unittest.TestCase):
    def test_size_abi_and_debug_symbol_regressions_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "fixture.c"
            source.write_text("int d110_size_fixture(void) { return 1; }\n")
            binary = root / "fixture.so"
            subprocess.run(["cc", "-shared", "-fPIC", "-g", "-o", binary, source], check=True)
            debug = binary.read_bytes()
            subprocess.run(["strip", "--strip-all", binary], check=True)
            stripped = binary.read_bytes()
            apk = root / "test.apk"

            def package(data, other_abi=False):
                with zipfile.ZipFile(apk, "w") as archive:
                    archive.writestr("lib/arm64-v8a/libniimbot_d110.so", data)
                    if other_abi:
                        archive.writestr("lib/x86_64/libother.so", data)

            package(stripped)
            size, native = apk_checks.inspect_apk(apk, "arm64-v8a")
            self.assertEqual(native, len(stripped))
            self.assertEqual(size, apk.stat().st_size)
            with self.assertRaisesRegex(ValueError, "APK limit"):
                apk_checks.inspect_apk(apk, "arm64-v8a", maximum=1)
            package(debug)
            with self.assertRaisesRegex(ValueError, "debug symbols"):
                apk_checks.inspect_apk(apk, "arm64-v8a")
            package(stripped, other_abi=True)
            with self.assertRaisesRegex(ValueError, "expected only arm64-v8a"):
                apk_checks.inspect_apk(apk, "arm64-v8a")


if __name__ == "__main__":
    unittest.main()
