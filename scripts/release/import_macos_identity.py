"""Import a Developer ID private key into a temporary GitHub Actions keychain."""

import base64
import os
from pathlib import Path
import secrets
import shlex
import subprocess


def main():
    names = ("CERTIFICATE_BASE64", "CERTIFICATE_PASSWORD", "SIGNING_IDENTITY", "SIGNING_TEAM_ID")
    missing = [name for name in names if not os.environ.get(name)]
    if missing:
        raise SystemExit("Configure MACOS_SIGNING_CERTIFICATE_BASE64, MACOS_SIGNING_CERTIFICATE_PASSWORD, MACOS_SIGNING_IDENTITY and MACOS_SIGNING_TEAM_ID before publishing macOS releases")
    from macos_signing import signing_configuration
    identity, team = signing_configuration({"ECHORA_MACOS_SIGN_IDENTITY": os.environ["SIGNING_IDENTITY"],
                                            "ECHORA_MACOS_SIGN_TEAM_ID": os.environ["SIGNING_TEAM_ID"]})
    root = Path(os.environ["RUNNER_TEMP"])
    certificate = root / "echora-signing.p12"
    keychain = root / "echora-release-signing.keychain-db"
    password = secrets.token_urlsafe(32)
    # Register cleanup before importing, including failed imports.
    with open(os.environ["GITHUB_ENV"], "a") as output:
        output.write(f"ECHORA_SIGNING_KEYCHAIN={keychain}\n")
    try:
        certificate.write_bytes(base64.b64decode(os.environ["CERTIFICATE_BASE64"], validate=True))
        certificate.chmod(0o600)
        def security(*arguments):
            result = subprocess.run(["security", *map(str, arguments)], capture_output=True)
            if result.returncode:
                # CalledProcessError prints argv, including import passwords.
                raise SystemExit(f"macOS signing keychain operation failed: {arguments[0]}")
        security("create-keychain", "-p", password, keychain)
        security("set-keychain-settings", "-lut", "21600", keychain)
        security("unlock-keychain", "-p", password, keychain)
        security("import", certificate, "-k", keychain, "-P", os.environ["CERTIFICATE_PASSWORD"], "-T", "/usr/bin/codesign")
        security("set-key-partition-list", "-S", "apple-tool:,apple:,codesign:", "-s", "-k", password, keychain)
        existing = subprocess.run(["security", "list-keychains", "-d", "user"], check=True, capture_output=True, text=True).stdout
        security("list-keychains", "-d", "user", "-s", keychain, *shlex.split(existing))
        with open(os.environ["GITHUB_ENV"], "a") as output:
            output.write(f'ECHORA_MACOS_SIGN_IDENTITY={identity}\n')
            output.write(f'ECHORA_MACOS_SIGN_TEAM_ID={team}\n')
    finally:
        certificate.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
