"""Sign releases with Apple's default certificate-bound designated requirement."""

import os
import re
import subprocess


def signing_configuration(environment=None):
    environment = os.environ if environment is None else environment
    identity = environment.get("ECHORA_MACOS_SIGN_IDENTITY", "").strip()
    team = environment.get("ECHORA_MACOS_SIGN_TEAM_ID", "").strip()
    required = environment.get("ECHORA_REQUIRE_MACOS_SIGNATURE", "") == "1"
    if identity or team:
        if not identity or identity == "-" or any(c in identity for c in "\r\n") or not re.fullmatch(r"[A-Z0-9]{10}", team):
            raise ValueError("macOS signing needs a Developer ID identity and its 10-character Team ID")
    elif required:
        raise ValueError("macOS releases require a stable Developer ID signature; configure the macOS signing secrets")
    return identity, team


def sign(app, configuration):
    identity, team = configuration
    if identity:
        subprocess.run(["codesign", "--force", "--sign", identity,
                        "--options", "runtime", "--timestamp", str(app)], check=True)
    else:
        print("Development package: ad-hoc signatures change identity across builds; Screen Recording access may need renewal.", flush=True)
        subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
    verify(app, team)


def verify(app, team):
    command = ["codesign", "--verify", "--deep", "--strict"]
    if team:
        # Validate the Apple anchor, Team ID and bundle ID without replacing
        # codesign's default designated requirement with an identifier-only rule.
        command += ["--test-requirement", f'=anchor apple generic and certificate leaf[subject.OU] = "{team}" and identifier "io.github.rita152.Echora"']
    subprocess.run([*command, str(app)], check=True)
    result = subprocess.run(["codesign", "-dvv", "-r-", str(app)], check=True, text=True, capture_output=True)
    details = result.stdout + result.stderr
    if team and (f"TeamIdentifier={team}\n" not in details
                 or "Authority=Developer ID Application:" not in details
                 or "Signature=adhoc" in details):
        raise ValueError("macOS installer is not signed with the expected Developer ID Application certificate")
    print(details, flush=True)
