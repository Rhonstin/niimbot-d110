from pathlib import Path
import tempfile
import unittest

from release import ARTIFACTS, DESKTOP, collect_assets, validate_run


class ReleaseTest(unittest.TestCase):
    def test_only_matching_successful_builds_are_releasable(self):
        request = {"tag": "v0.1.0", "commit": "a" * 40, "run_id": 42}
        run = {
            "status": "completed", "conclusion": "success", "head_sha": "a" * 40,
            "id": 42, "path": ".github/workflows/ci.yml",
        }
        validate_run(request, run)
        for key, value in [
            ("head_sha", "b" * 40), ("conclusion", "failure"),
            ("status", "in_progress"), ("id", 43), ("path", "other.yml"),
        ]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_run(request, {**run, key: value})
        for tag in ["../v0.1.0", "v0.1", "v0.1.0 --draft"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                validate_run({**request, "tag": tag}, run)

    def test_all_architectures_are_required_and_names_stay_distinct(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for artifact in ARTIFACTS:
                (root / artifact).mkdir()
            for target, (_, extensions) in DESKTOP.items():
                for extension in extensions:
                    (root / f"d110-studio-{target}" / f"installer{extension}").write_bytes(b"data")
            android = root / "d110-studio-android"
            for abi in ["arm64", "x86_64"]:
                (android / f"app-{abi}-release.apk").write_bytes(b"apk")
            assets = collect_assets(root, "0.1.0")
            self.assertEqual(len(assets), 12)
            self.assertEqual(len({name for _, name in assets}), 12)
            missing = android / "app-arm64-release.apk"
            missing.unlink()
            with self.assertRaisesRegex(ValueError, "Android arm64"):
                collect_assets(root, "0.1.0")
            missing.write_bytes(b"apk")
            linux = root / "d110-studio-x86_64-unknown-linux-gnu"
            (linux / "duplicate.deb").write_bytes(b"duplicate")
            with self.assertRaisesRegex(ValueError, "linux-x64 .deb"):
                collect_assets(root, "0.1.0")


if __name__ == "__main__":
    unittest.main()
