"""Decrypt verified Adobe build inputs; never print or publish their contents."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import zipfile


ROOT = Path(__file__).resolve().parent.parent / ".sdk"
HASHES = {
    "win": "af0eb59d27e952636bf0fd7848be655f959097bcdee3d69a0786a756d14f73be",
    "mac": "f8ac26732ce69841c9ce6d8bd07b62939e9d7c20d07cbce118f733b938a55054",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", nargs="?", choices=HASHES)
    parser.add_argument("--clean", action="store_true")
    args = parser.parse_args()
    if args.clean:
        # Delete only the CI-created directory and plaintext ZIPs, never local SDKs.
        extracted = ROOT / "extracted"
        if extracted.is_dir():
            shutil.rmtree(extracted)
        for platform in HASHES:
            (ROOT / f"adobe_photoshop_sdk_2026_{platform}_v2.zip").unlink(missing_ok=True)
        return
    if not args.platform:
        parser.error("platform is required unless --clean is used")
    password = os.environ.get("PHOTOSHOP_SDK_PASSPHRASE")
    if not password:
        raise SystemExit("Set the PHOTOSHOP_SDK_PASSPHRASE Actions secret to build native plugins")
    gpg = shutil.which("gpg")
    if not gpg and os.name == "nt":
        candidate = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/usr/bin/gpg.exe"
        if candidate.is_file():
            gpg = str(candidate)
    if not gpg:
        raise SystemExit("GnuPG is required to decrypt the SDK")
    archive = ROOT / f"adobe_photoshop_sdk_2026_{args.platform}_v2.zip"
    subprocess.run([
        gpg, "--batch", "--yes", "--pinentry-mode", "loopback", "--passphrase-fd", "0",
        "--output", str(archive), "--decrypt", str(archive) + ".gpg",
    ], input=(password + "\n").encode(), check=True)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != HASHES[args.platform]:
        archive.unlink()
        raise SystemExit("SDK SHA-256 mismatch")
    with zipfile.ZipFile(archive) as sdk:
        sdk.extractall(ROOT / "extracted")
    archive.unlink()
    print("SDK decrypted and SHA-256 verified")


if __name__ == "__main__":
    main()
