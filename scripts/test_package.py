"""Packaging checks without compilers, installed applications or system changes.

Run with: python3 -m unittest discover -s scripts -p 'test_*.py'
"""
import contextlib
import io
import json
import os
from pathlib import Path
import plistlib
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import package


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sannygit-package-tests-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "build with spaces/sannygit"
        self.binary.parent.mkdir()
        self.binary.write_bytes(b"application executable")
        self.binary.chmod(0o755)
        self.output = self.root / "package"
        self.prefix = self.root / "install with spaces"

    def test_macos_bundle_has_version_icon_and_executable_and_can_be_updated(self):
        artifact = package.package_native(self.binary, self.output, "darwin", "1.2.3")
        with (artifact / "Contents/Info.plist").open("rb") as file:
            info = plistlib.load(file)
        self.assertEqual(info["CFBundleVersion"], "1.2.3")
        executable = artifact / "Contents/MacOS" / info["CFBundleExecutable"]
        self.assertEqual(executable.read_bytes(), self.binary.read_bytes())
        if os.name != "nt":
            self.assertTrue(executable.stat().st_mode & stat.S_IXUSR)
        self.assertTrue((artifact / "Contents/Resources" / info["CFBundleIconFile"]).is_file())
        self.assertTrue((artifact / "Contents/Resources/LICENSE").is_file())
        installed = package.install_native(artifact, self.prefix, "darwin")
        unrelated = self.prefix / "unrelated.txt"
        unrelated.write_text("preserve me")
        self.binary.write_bytes(b"updated executable")
        package.package_native(self.binary, self.output, "darwin", "1.2.4")
        package.install_native(artifact, self.prefix, "darwin")
        self.assertEqual((installed / "Contents/MacOS/sannygit").read_bytes(), b"updated executable")
        self.assertEqual(unrelated.read_text(), "preserve me")

    def test_linux_install_contains_desktop_entry_icons_and_executable(self):
        artifact = package.package_native(self.binary, self.output, "linux", "1.2.3")
        (artifact / "unrelated.txt").write_text("not an application artifact")
        self.prefix.mkdir(mode=0o700)
        original_mode = stat.S_IMODE(self.prefix.stat().st_mode)
        installed = package.install_native(artifact, self.prefix, "linux")
        self.assertEqual(installed.read_bytes(), self.binary.read_bytes())
        if os.name != "nt":
            self.assertTrue(installed.stat().st_mode & stat.S_IXUSR)
        self.assertEqual(stat.S_IMODE(self.prefix.stat().st_mode), original_mode)
        self.assertFalse((self.prefix / "unrelated.txt").exists())
        self.assertTrue((self.prefix / "share/licenses/sannygit/LICENSE").is_file())
        self.assertIn("Exec=sannygit", (self.prefix / "share/applications/SannyGit.desktop").read_text())
        for size, extension in [("256x256", "png"), ("scalable", "svg")]:
            self.assertTrue((self.prefix / f"share/icons/hicolor/{size}/apps/sannygit.{extension}").is_file())

    def test_windows_install_does_not_touch_other_files(self):
        artifact = package.package_native(self.binary, self.output, "win32", "1.2.3")
        self.prefix.mkdir()
        unrelated = self.prefix / "unrelated.txt"
        unrelated.write_text("preserve me")
        installed = package.install_native(artifact, self.prefix, "win32")
        self.assertEqual(installed.name, "SannyGit.exe")
        self.assertTrue((self.prefix / "LICENSE").is_file())
        self.assertEqual(installed.read_bytes(), self.binary.read_bytes())
        self.assertEqual(unrelated.read_text(), "preserve me")

    def test_install_prefix_defaults_and_empty_explicit_prefix(self):
        with patch.dict(os.environ, {"LOCALAPPDATA": str(self.root)}):
            self.assertEqual(package.default_install_prefix("win32"), self.root / "Programs/SannyGit")
        self.assertEqual(package.default_install_prefix("darwin"), Path.home() / "Applications")
        self.assertEqual(package.default_install_prefix("linux"), Path.home() / ".local")
        self.assertIsNone(package.parse_args([]).install)
        self.assertEqual(package.parse_args(["--install"]).install, "")
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            package.parse_args(["--install="])

    def metadata(self):
        return json.dumps({"target_directory": str(self.root / "custom target"), "packages": [
            {"name": "sannygit", "id": "application-id", "version": "1.2.3"}
        ]})

    def build_result(self):
        return subprocess.CompletedProcess([], 0, stdout=json.dumps({
            "reason": "compiler-artifact", "package_id": "application-id",
            "target": {"name": "sannygit"}, "executable": str(self.binary),
        }) + "\n")

    def test_release_tests_and_custom_target_executable_are_used_before_install(self):
        events = []

        def run(arguments, **kwargs):
            events.append(arguments)
            self.assertEqual(kwargs["cwd"], package.ROOT)
            if "test" in arguments:
                self.assertFalse((self.root / "custom target/package").exists())
                self.assertFalse(self.prefix.exists())
            return self.build_result()

        with patch.object(package.subprocess, "check_output", return_value=self.metadata()), \
                patch.object(package.subprocess, "run", side_effect=run), \
                patch.object(package.sys, "platform", "linux"), \
                contextlib.redirect_stdout(io.StringIO()):
            package.main(["--tests", f"--install={self.prefix}"])
        self.assertEqual(events, [
            ["cargo", "build", "--release", "--message-format=json-render-diagnostics"],
            ["cargo", "test", "--release"],
        ])
        self.assertEqual((self.prefix / "bin/sannygit").read_bytes(), self.binary.read_bytes())

    def test_failed_tests_do_not_package_or_install(self):
        def run(arguments, **kwargs):
            if "test" in arguments:
                raise subprocess.CalledProcessError(1, arguments)
            return self.build_result()

        with patch.object(package.subprocess, "check_output", return_value=self.metadata()), \
                patch.object(package.subprocess, "run", side_effect=run), \
                patch.object(package.sys, "platform", "linux"), \
                contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaises(subprocess.CalledProcessError):
            package.main(["--tests", f"--install={self.prefix}"])
        self.assertFalse((self.root / "custom target/package").exists())
        self.assertFalse(self.prefix.exists())

    def test_missing_build_artifact_is_an_error(self):
        with patch.object(package.subprocess, "check_output", return_value=self.metadata()), \
                patch.object(package.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, stdout="")), \
                patch.object(package.sys, "platform", "linux"), \
                contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(RuntimeError, "executable"):
            package.main([])
        self.assertFalse((self.root / "custom target/package").exists())

    @unittest.skipIf(os.name == "nt", "Bash entry point is for Linux and macOS")
    def test_shell_help_and_invalid_options_exit_before_dependency_setup(self):
        script = package.ROOT / "build.sh"
        for arguments, expected in [(["--help"], 0), (["--unknown"], 1), (["--install="], 1)]:
            result = subprocess.run(["/bin/bash", str(script), *arguments], cwd=self.root,
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, expected, result.stderr)
            self.assertNotIn("Installing", result.stdout)


if __name__ == "__main__":
    unittest.main()
