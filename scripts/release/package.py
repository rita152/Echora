#!/usr/bin/env python3
"""Build and smoke-test a native installer from an Echora portable payload."""

import argparse
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tempfile
import time


def run(*command, **kwargs):
    print("+", *map(str, command), flush=True)
    return subprocess.run(list(map(str, command)), check=True, **kwargs)


def diagnostics(executable, cwd, expected_assets):
    result = run(executable, "--print-diagnostics", cwd=cwd, capture_output=True, text=True)
    print(result.stdout, flush=True)
    expected = str(expected_assets.resolve())
    assert f"assets base: {expected}" in result.stdout.replace("\\\\?\\", ""), result.stdout
    assert expected_assets.joinpath("icons").is_dir()


def macos(payload, output, version, scratch):
    volume = scratch / "volume"
    app = volume / "Echora.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    shutil.copy2(payload / "gpui-chat-clone", contents / "MacOS/gpui-chat-clone")
    shutil.copytree(payload / "assets", contents / "Resources/assets")
    # Preserve WebKit's permissions and transport settings from the app template.
    with (Path(__file__).parents[1] / "gpui_capture_info.plist").open("rb") as source:
        info = plistlib.load(source)
    info.update(
        CFBundleIdentifier="io.github.rita152.Echora",
        CFBundleName="Echora",
        CFBundleDisplayName="Echora",
        CFBundleShortVersionString=version.split("-", 1)[0].split("+", 1)[0],
        CFBundleVersion=version.split("-", 1)[0].split("+", 1)[0],
        CFBundleGetInfoString=f"Echora {version}",
        LSMinimumSystemVersion="11.0",
    )
    with (contents / "Info.plist").open("wb") as target:
        plistlib.dump(info, target, sort_keys=False)
    run("codesign", "--force", "--sign", "-", app)
    run("codesign", "--verify", "--deep", "--strict", app)
    (volume / "Applications").symlink_to("/Applications")
    installer = output / f"Echora-{version}-macos-arm64.dmg"
    run("hdiutil", "create", "-volname", "Echora", "-srcfolder", volume,
        "-format", "UDZO", "-ov", installer)
    run("hdiutil", "verify", installer)
    mount = scratch / "mounted"
    mount.mkdir()
    run("hdiutil", "attach", installer, "-nobrowse", "-readonly", "-mountpoint", mount)
    try:
        assert (mount / "Applications").is_symlink()
        installed = scratch / "installed/Echora.app"
        shutil.copytree(mount / "Echora.app", installed)
        run("codesign", "--verify", "--deep", "--strict", installed)
        diagnostics(installed / "Contents/MacOS/gpui-chat-clone", scratch,
                    installed / "Contents/Resources/assets")
    finally:
        run("hdiutil", "detach", mount)


def linux(payload, output, version, scratch):
    root = scratch / "deb"
    app = root / "usr/lib/echora"
    shutil.copytree(payload, app)
    (root / "usr/bin").mkdir(parents=True)
    launcher = root / "usr/bin/echora"
    launcher.write_text('#!/bin/sh\nexec /usr/lib/echora/gpui-chat-clone "$@"\n')
    launcher.chmod(0o755)
    desktop = root / "usr/share/applications/io.github.rita152.Echora.desktop"
    desktop.parent.mkdir(parents=True)
    desktop.write_text(
        "[Desktop Entry]\nType=Application\nName=Echora\n"
        "Comment=Native desktop workspace for Codex\nExec=echora\n"
        "Icon=applications-development\nTerminal=false\nCategories=Development;\n"
        "StartupNotify=true\n"
    )
    run("desktop-file-validate", desktop)
    debian = scratch / "debian"
    debian.mkdir()
    (debian / "control").write_text(
        "Source: echora\nSection: devel\nPriority: optional\n"
        "Maintainer: Echora contributors <rita152@users.noreply.github.com>\n\n"
        "Package: echora\nArchitecture: amd64\nDescription: Native desktop workspace for Codex\n"
    )
    result = run("dpkg-shlibdeps", "-O", f"-e{app / 'gpui-chat-clone'}",
                 cwd=scratch, capture_output=True, text=True)
    print(result.stdout, result.stderr, flush=True)
    depends = next(line.removeprefix("shlibs:Depends=") for line in result.stdout.splitlines()
                   if line.startswith("shlibs:Depends="))
    # GPUI also loads the graphics backends dynamically, outside the ELF NEEDED list.
    depends += (", libvulkan1, libegl1, libwayland-client0, libwayland-cursor0, "
                "libwayland-egl1, libxkbcommon0, libxkbcommon-x11-0, libx11-xcb1, "
                "fonts-dejavu-core")
    control = root / "DEBIAN/control"
    control.parent.mkdir()
    control.write_text(
        f"Package: echora\nVersion: {version.replace('-', '~', 1)}\n"
        "Architecture: amd64\nSection: devel\nPriority: optional\n"
        "Maintainer: Echora contributors <rita152@users.noreply.github.com>\n"
        "Homepage: https://github.com/rita152/Echora\n"
        f"Depends: {depends}\n"
        "Description: Native desktop workspace for Codex\n"
        " Development preview. Requires a separately configured Codex CLI.\n"
    )
    installer = output / f"Echora-{version}-linux-amd64.deb"
    run("dpkg-deb", "--build", "--root-owner-group", root, installer)
    run("dpkg-deb", "--info", installer)
    run("sudo", "apt-get", "install", "--no-install-recommends", "-y", installer)
    try:
        diagnostics(Path("/usr/bin/echora"), scratch, Path("/usr/lib/echora/assets"))
        run("desktop-file-validate", "/usr/share/applications/io.github.rita152.Echora.desktop")
    finally:
        run("sudo", "dpkg", "--remove", "echora")
    assert not Path("/usr/bin/echora").exists()


def windows(payload, output, version, scratch):
    program_files = Path(os.environ["ProgramFiles(x86)"])
    iscc = shutil.which("ISCC.exe")
    if not iscc:
        iscc = next(program_files.glob("Inno Setup */ISCC.exe"), None)
    if not iscc:
        raise SystemExit("Inno Setup is required to build the Windows installer")
    # App-local MSVC runtime DLLs let the per-user installer work without a
    # separate administrator-level Visual C++ Redistributable installation.
    vswhere = program_files / "Microsoft Visual Studio/Installer/vswhere.exe"
    visual_studio = run(vswhere, "-latest", "-products", "*", "-property", "installationPath",
                        capture_output=True, text=True).stdout.strip()
    runtimes = list(Path(visual_studio).glob("VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT"))
    if not runtimes:
        raise SystemExit("Visual Studio x64 redistributable CRT files were not found")
    runtime = max(runtimes, key=lambda p: tuple(map(int, re.findall(r"\d+", str(p)))))
    for dll in runtime.glob("*.dll"):
        shutil.copy2(dll, payload / dll.name)
    for required in ("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"):
        assert (payload / required).is_file(), required
    environment = dict(os.environ, ECHORA_VERSION=version, ECHORA_PAYLOAD=str(payload),
                       ECHORA_OUTPUT=str(output), ECHORA_NUMERIC_VERSION=version.split("-", 1)[0].split("+", 1)[0])
    run(iscc, Path(__file__).with_name("windows.iss"), env=environment)
    installer = output / f"Echora-{version}-windows-x86_64-setup.exe"
    installed = scratch / "installed"
    run(installer, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", f"/DIR={installed}")
    try:
        diagnostics(installed / "gpui-chat-clone.exe", scratch, installed / "assets")
        shortcut = Path(os.environ["APPDATA"]) / "Microsoft/Windows/Start Menu/Programs/Echora.lnk"
        assert shortcut.is_file(), shortcut
    finally:
        run(installed / "unins000.exe", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART")
    for _ in range(30):
        if not (installed / "gpui-chat-clone.exe").exists():
            break
        time.sleep(1)
    assert not (installed / "gpui-chat-clone.exe").exists()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", required=True, choices=("macos", "windows", "linux"))
    args = parser.parse_args()
    payload = args.payload.resolve()
    output = args.output.resolve()
    assert payload.joinpath("assets/icons").is_dir(), "Payload must contain application assets"
    output.mkdir(parents=True, exist_ok=True)
    # Linux and Windows install/uninstall smoke tests are intended for disposable CI runners.
    if args.platform in ("linux", "windows") and os.environ.get("GITHUB_ACTIONS") != "true":
        raise SystemExit("Run installer tests on a disposable GitHub Actions runner")
    with tempfile.TemporaryDirectory(prefix="echora-installer-") as temporary:
        builders = {"macos": macos, "windows": windows, "linux": linux}
        builders[args.platform](payload, output, args.version, Path(temporary).resolve())


if __name__ == "__main__":
    main()
