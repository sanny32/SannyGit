"""Run with python3 .github/macos/test_keychain_entitlements.py."""

import copy
import datetime
import runpy
import unittest
from pathlib import Path


keychain_entitlements = runpy.run_path(
    str(Path(__file__).with_name("keychain-entitlements.py"))
)["keychain_entitlements"]


class KeychainEntitlementsTests(unittest.TestCase):
    def setUp(self):
        self.profile = {
            "Platform": ["OSX"],
            "ExpirationDate": datetime.datetime.now(datetime.timezone.utc)
            + datetime.timedelta(days=1),
            "ApplicationIdentifierPrefix": ["PREFIX1234"],
            "TeamIdentifier": ["TEAM123456"],
            "Entitlements": {
                "com.apple.application-identifier": "PREFIX1234.io.github.sanny32.SannyGit",
                "com.apple.developer.team-identifier": "TEAM123456",
                "keychain-access-groups": ["PREFIX1234.*"],
                "com.apple.security.get-task-allow": True,
            },
        }

    def derive(self):
        return keychain_entitlements(self.profile, "io.github.sanny32.SannyGit")

    def test_only_keychain_entitlements_are_copied(self):
        before = copy.deepcopy(self.profile)
        self.assertEqual(self.derive(), {
            "com.apple.application-identifier": "PREFIX1234.io.github.sanny32.SannyGit",
            "com.apple.developer.team-identifier": "TEAM123456",
            "keychain-access-groups": ["PREFIX1234.io.github.sanny32.SannyGit"],
        })
        self.assertEqual(self.profile, before)

    def test_wildcard_profile_produces_concrete_app_identifier(self):
        self.profile["Entitlements"]["com.apple.application-identifier"] = "PREFIX1234.*"
        self.assertEqual(self.derive()["com.apple.application-identifier"],
                         "PREFIX1234.io.github.sanny32.SannyGit")

    def test_debugging_is_only_enabled_on_request_and_does_not_require_profile_authorization(self):
        self.assertNotIn("com.apple.security.get-task-allow", self.derive())
        entitlements = keychain_entitlements(self.profile, "io.github.sanny32.SannyGit", debug=True)
        self.assertTrue(entitlements["com.apple.security.get-task-allow"])
        del self.profile["Entitlements"]["com.apple.security.get-task-allow"]
        entitlements = keychain_entitlements(self.profile, "io.github.sanny32.SannyGit", debug=True)
        self.assertTrue(entitlements["com.apple.security.get-task-allow"])

    def test_rejects_another_app_or_keychain_group(self):
        for key in ["com.apple.application-identifier", "keychain-access-groups"]:
            with self.subTest(key=key):
                previous = self.profile["Entitlements"][key]
                self.profile["Entitlements"][key] = (
                    ["PREFIX1234.other"] if isinstance(previous, list) else "PREFIX1234.other"
                )
                with self.assertRaises(ValueError):
                    self.derive()
                self.profile["Entitlements"][key] = previous

    def test_rejects_expired_profile(self):
        self.profile["ExpirationDate"] = datetime.datetime(2000, 1, 1)
        with self.assertRaisesRegex(ValueError, "expired"):
            self.derive()

    def test_rejects_other_platform_and_team(self):
        self.profile["Platform"] = ["iOS"]
        with self.assertRaisesRegex(ValueError, "macOS"):
            self.derive()
        self.profile["Platform"] = ["OSX"]
        self.profile["TeamIdentifier"] = ["OTHER12345"]
        with self.assertRaisesRegex(ValueError, "team"):
            self.derive()


if __name__ == "__main__":
    unittest.main()
