"""Release identity checks; real codesign verification on macOS."""
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest

import macos_signing


class SigningConfigurationTests(unittest.TestCase):
    def test_tagged_release_cannot_fall_back_to_adhoc(self):
        with self.assertRaisesRegex(ValueError, "stable Developer ID"):
            macos_signing.signing_configuration({"ECHORA_REQUIRE_MACOS_SIGNATURE": "1"})
        self.assertEqual(macos_signing.signing_configuration({}), ("", ""))

    def test_partial_or_adhoc_identity_cannot_pass_as_release_signing(self):
        for identity, team in [("-", "ABCDEFGHIJ"), ("certificate", ""), ("", "ABCDEFGHIJ"), ("certificate", "invalid")]:
            with self.subTest(identity=identity, team=team), self.assertRaises(ValueError):
                macos_signing.signing_configuration({"ECHORA_MACOS_SIGN_IDENTITY": identity,
                                                    "ECHORA_MACOS_SIGN_TEAM_ID": team})

    def test_missing_ci_secrets_fail_without_creating_a_keychain(self):
        result = subprocess.run([sys.executable, str(Path(__file__).with_name("import_macos_identity.py"))],
                                env={k: v for k, v in os.environ.items() if k not in
                                     {"CERTIFICATE_BASE64", "CERTIFICATE_PASSWORD", "SIGNING_IDENTITY", "SIGNING_TEAM_ID"}},
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("before publishing", result.stderr)


@unittest.skipUnless(sys.platform == "darwin", "Requires native macOS codesign")
class NativeIdentityTests(unittest.TestCase):
    def test_valid_adhoc_updates_fail_the_old_identity_and_release_requirement(self):
        with tempfile.TemporaryDirectory(prefix="echora-identity-test-") as directory:
            root = Path(directory)
            builds = []
            requirements = []
            for version in (1, 2):
                app = root / str(version) / "Echora.app"
                binary = app / "Contents/MacOS/fixture"
                binary.parent.mkdir(parents=True)
                source = app.parent / "fixture.c"
                source.write_text(f"int main(void) {{ return {version}; }}\n")
                subprocess.run(["xcrun", "clang", str(source), "-o", str(binary)], check=True, capture_output=True)
                (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
                    "CFBundleIdentifier": "io.github.rita152.Echora", "CFBundleExecutable": "fixture"}))
                macos_signing.sign(app, ("", ""))
                details = subprocess.run(["codesign", "-d", "-r-", str(app)], check=True, capture_output=True, text=True)
                requirements.append(next(line.split("designated => ", 1)[1]
                                         for line in (details.stdout + details.stderr).splitlines() if "designated => " in line))
                builds.append(app)
            self.assertNotEqual(*requirements)
            result = subprocess.run(["codesign", "--verify", "--test-requirement", "=" + requirements[0], str(builds[1])], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"failed to satisfy", result.stderr)
            with self.assertRaises(subprocess.CalledProcessError):
                macos_signing.verify(builds[1], "ABCDEFGHIJ")


if __name__ == "__main__":
    unittest.main()
