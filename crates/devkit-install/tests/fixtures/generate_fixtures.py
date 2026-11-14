"""Generates the committed happy-path fixture archives for the P3-02
archive-adapter tests (crates/devkit-install/tests/fixtures/).

Run from this directory:

    python generate_fixtures.py

Attack archives (traversal, symlinks, bombs) are NOT committed: they are
crafted in-test with the same zip/tar crates the adapter reads, so their
malformed structure is explicit in the test code. Only the benign
fixtures need to be stable across machines, hence this script.
"""

import io
import os
import tarfile
import zipfile

# Deterministic payload set: content identical for zip and tar fixtures.
FILES = {
    "app/bin/tool.exe": b"MZ" + b"A" * 100,
    "app/bin/readme.txt": b"DevKit fixture archive\n",
    "app/lib/core.dat": bytes(range(256)) * 16,
    "app/README.md": b"# fixture archive\n",
}


def make_zip(path: str) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in FILES.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.external_attr = 0o644 << 16
            archive.writestr(info, data)


def make_tar_gz(path: str) -> None:
    with tarfile.open(path, "w:gz") as archive:
        for name, data in FILES.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))


def main() -> None:
    here = os.path.dirname(os.path.abspath(__file__))
    make_zip(os.path.join(here, "happy.zip"))
    make_tar_gz(os.path.join(here, "happy.tar.gz"))
    print("wrote happy.zip and happy.tar.gz")


if __name__ == "__main__":
    main()
