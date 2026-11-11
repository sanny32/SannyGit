"""Derive only the Keychain entitlements authorized by a macOS profile."""

import argparse
import datetime
import fnmatch
import plistlib
import subprocess
import sys
from pathlib import Path


def keychain_entitlements(profile, bundle_id, debug=False):
    if "OSX" not in profile.get("Platform", []):
        raise ValueError("The provisioning profile must be for macOS.")
    expires = profile.get("ExpirationDate")
    if not isinstance(expires, datetime.datetime) or expires.replace(
        tzinfo=datetime.timezone.utc
    ) <= datetime.datetime.now(datetime.timezone.utc):
        raise ValueError("The provisioning profile has expired or has no expiration date.")
    allowed = profile.get("Entitlements", {})
    pattern = allowed.get("com.apple.application-identifier", "")
    prefixes = profile.get("ApplicationIdentifierPrefix", [])
    app_id = next(
        (f"{prefix}.{bundle_id}" for prefix in prefixes
         if fnmatch.fnmatchcase(f"{prefix}.{bundle_id}", pattern)),
        None,
    )
    if app_id is None:
        raise ValueError("The provisioning profile does not authorize this bundle identifier.")
    groups = allowed.get("keychain-access-groups", [])
    if not any(fnmatch.fnmatchcase(app_id, group) for group in groups):
        raise ValueError("The provisioning profile does not authorize the app's Keychain group.")
    team = allowed.get("com.apple.developer.team-identifier")
    if not team or team not in profile.get("TeamIdentifier", []):
        raise ValueError("The provisioning profile has no matching team identifier.")
    entitlements = {
        "com.apple.application-identifier": app_id,
        "com.apple.developer.team-identifier": team,
        "keychain-access-groups": [app_id],
    }
    if debug:
        # This entitlement is unrestricted on macOS; only debug builds claim it.
        entitlements["com.apple.security.get-task-allow"] = True
    return entitlements


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile")
    parser.add_argument("info")
    parser.add_argument("output")
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    try:
        decoded = subprocess.run(
            ["security", "cms", "-D", "-i", args.profile],
            check=True, capture_output=True,
        ).stdout
        profile = plistlib.loads(decoded)
        info = plistlib.loads(Path(args.info).read_bytes())
        entitlements = keychain_entitlements(profile, info["CFBundleIdentifier"], args.debug)
        Path(args.output).write_bytes(plistlib.dumps(entitlements))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(f"Cannot configure Touch ID signing: {error}")


if __name__ == "__main__":
    main()
