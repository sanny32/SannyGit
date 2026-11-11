"""Exercise Unix build/install behavior and CI version mapping without system changes."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent


@unittest.skipIf(os.name == "nt", "Unix shell tests run on Linux and macOS")
class BuildScriptTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sannygit-build-tests-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.tools = self.root / "tools"
        self.tools.mkdir()
        self.binary = self.root / "custom target with spaces/release/sannygit"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b"test executable")
        self.binary.chmod(0o755)
        self.prefix = self.root / "install with spaces"
        self.log = self.root / "commands.jsonl"
        self.env = dict(os.environ, PATH=f"{self.tools}{os.pathsep}{os.environ['PATH']}",
                        TEST_BINARY=str(self.binary), TEST_LOG=str(self.log), TEST_OS="Linux")
        for name in ["cargo", "rustc", "uname", "pkg-config", "xcode-select", "codesign"]:
            tool = self.tools / name
            tool.write_text(f"#!{sys.executable}\n" + textwrap.dedent('''\
                import json, os, sys
                from pathlib import Path
                name = Path(sys.argv[0]).name
                with open(os.environ['TEST_LOG'], 'a') as file:
                    file.write(json.dumps([name, *sys.argv[1:]]) + '\\n')
                if name == 'uname':
                    print(os.environ['TEST_OS'])
                elif name == 'rustc':
                    print('rustc 1.99.1 (test fixture)')
                elif name == 'cargo':
                    if sys.argv[1] == 'build':
                        if os.environ.get('TEST_BUILD_FAIL'): sys.exit(1)
                        print(json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'sannygit'},
                                          'executable': os.environ['TEST_BINARY']}))
                    elif sys.argv[1] == 'test':
                        if os.environ.get('TEST_TEST_FAIL'): sys.exit(1)
                    elif sys.argv[1] == 'metadata':
                        print(json.dumps({'packages': [{'name': 'sannygit',
                                                        'version': os.environ.get('TEST_VERSION', '1.2.3')}]}))
                '''))
            tool.chmod(0o755)
        # Helpers must be visible even when the test runner uses a versioned Python.
        for name in ['python3', 'python']:
            (self.tools / name).symlink_to(sys.executable)
        for name in ['cc', 'git', 'curl']:
            (self.tools / name).symlink_to(shutil.which('true') or '/usr/bin/true')

    def run_build(self, *arguments):
        return subprocess.run(['/bin/bash', str(ROOT / 'build.sh'), *arguments],
                              cwd=self.root, env=self.env, capture_output=True, text=True)

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_help_invalid_options_and_empty_prefix_exit_before_setup(self):
        for args, expected in [(['--help'], 0), (['--unknown'], 1), (['--install='], 1)]:
            result = self.run_build(*args)
            self.assertEqual(result.returncode, expected, result.stderr)
            self.assertEqual(self.commands(), [])

    def test_default_build_does_not_install_and_honors_cargo_executable_path(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(self.binary), result.stdout)
        self.assertIn(['cargo', 'build', '--release', '--message-format=json-render-diagnostics'], self.commands())
        self.assertFalse(self.prefix.exists())
        self.assertFalse(any(command[:2] == ['cargo', 'test'] for command in self.commands()))

    def test_debug_build_and_tests_use_the_development_profile(self):
        result = self.run_build('--debug', '--tests')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(['cargo', 'build', '--message-format=json-render-diagnostics'], self.commands())
        self.assertIn(['cargo', 'test'], self.commands())

    def test_linux_install_preserves_existing_files_and_directory_permissions(self):
        self.prefix.mkdir(mode=0o700)
        (self.prefix / 'bin').mkdir(mode=0o700)
        (self.prefix / 'unrelated').write_text('keep')
        result = self.run_build('--tests', f'--install={self.prefix}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(['cargo', 'test', '--release'], self.commands())
        self.assertEqual((self.prefix / 'bin/sannygit').read_bytes(), self.binary.read_bytes())
        self.assertTrue((self.prefix / 'bin/sannygit').stat().st_mode & stat.S_IXUSR)
        self.assertEqual(stat.S_IMODE((self.prefix / 'bin').stat().st_mode), 0o700)
        self.assertEqual((self.prefix / 'unrelated').read_text(), 'keep')
        for path in ['share/applications/io.github.sanny32.SannyGit.desktop',
                     'share/icons/hicolor/256x256/apps/sannygit.png',
                     'share/icons/hicolor/scalable/apps/sannygit.svg',
                     'share/licenses/sannygit/LICENSE', 'share/licenses/sannygit/SourceSans3-OFL.txt']:
            self.assertTrue((self.prefix / path).is_file(), path)

    def test_failed_build_or_tests_do_not_modify_installation(self):
        for variable in ['TEST_BUILD_FAIL', 'TEST_TEST_FAIL']:
            self.env[variable] = '1'
            result = self.run_build('--tests', f'--install={self.prefix}')
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.prefix.exists())
            del self.env[variable]

    def test_macos_bundle_installs_manifest_icon_licenses_and_signs(self):
        self.env['TEST_OS'] = 'Darwin'
        result = self.run_build(f'--install={self.prefix}')
        self.assertEqual(result.returncode, 0, result.stderr)
        bundle = self.prefix / 'SannyGit.app'
        with (bundle / 'Contents/Info.plist').open('rb') as file:
            info = plistlib.load(file)
        self.assertEqual(info['CFBundleIdentifier'], 'io.github.sanny32.SannyGit')
        self.assertEqual((bundle / 'Contents/MacOS' / info['CFBundleExecutable']).read_bytes(), self.binary.read_bytes())
        for name in ['sannygit.icns', 'LICENSE', 'SourceSans3-OFL.txt']:
            self.assertTrue((bundle / 'Contents/Resources' / name).is_file())
        built_bundle = self.binary.parent / 'SannyGit.app'
        self.assertIn(['codesign', '--force', '--sign', '-', str(built_bundle)], self.commands())
        self.assertIn(['codesign', '--verify', '--strict', str(bundle)], self.commands())

    def run_version(self, version, tag=None):
        action = (ROOT / '.github/actions/project-version/action.yml').read_text()
        script = textwrap.dedent(action.split('      run: |\n', 1)[1])
        output = self.root / 'version-output'
        env = dict(self.env, TEST_VERSION=version, GITHUB_OUTPUT=str(output), RUNNER_OS='Linux',
                   GITHUB_REF_TYPE='tag' if tag else 'branch', GITHUB_REF_NAME=tag or 'main')
        result = subprocess.run(['/bin/bash', '-e', '-o', 'pipefail', '-c', script],
                                cwd=self.root, env=env, capture_output=True, text=True)
        values = dict(line.split('=', 1) for line in output.read_text().splitlines()) if output.exists() else {}
        return result, values

    def test_ci_version_mapping_for_stable_and_prerelease_packages(self):
        result, values = self.run_version('1.2.3', 'v1.2.3')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values, dict(version='1.2.3', deb_version='1.2.3', rpm_version='1.2.3',
                                      rpm_release='1', file_version='1.2.3.0'))
        result, values = self.run_version('2.3.4-beta.2+build.7')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values['deb_version'], '2.3.4~beta.2+build.7')
        self.assertEqual(values['rpm_version'], '2.3.4')
        self.assertEqual(values['rpm_release'], '0.beta.2')
        self.assertEqual(values['file_version'], '2.3.4.0')

    def test_ci_rejects_tag_that_does_not_match_cargo_version(self):
        result, values = self.run_version('1.2.3', 'v1.2.4')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('does not match Cargo version', result.stdout)
        self.assertEqual(values, {})


if __name__ == '__main__':
    unittest.main()
