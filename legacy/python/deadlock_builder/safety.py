from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import tempfile


class BuildError(Exception):
    """An actionable error safe to show in the interface."""


def relative_asset(value: str) -> str:
    value = value.replace("\\", "/")
    p = PurePosixPath(value)
    if not value or p.is_absolute() or any(x in ("..", ".", "") for x in value.split("/")):
        raise BuildError(f"Unsafe game-relative asset path: {value!r}")
    if any(c in value for c in ':\x00\r\n*?"<>|'):
        raise BuildError(f"Invalid asset path: {value!r}")
    return str(p)


def inside(root: Path, relative: str) -> Path:
    root = root.resolve()
    p = (root / relative_asset(relative)).resolve()
    if not p.is_relative_to(root):
        raise BuildError(f"Path escapes workspace: {relative}")
    return p


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for data in iter(lambda: f.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def atomic_write(path: Path, data: bytes | str):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".builder-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data.encode("utf-8") if isinstance(data, str) else data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def write_json(path: Path, obj):
    atomic_write(path, json.dumps(obj, indent=2, ensure_ascii=False) + "\n")


def copy_tree(source: Path, target: Path):
    """Copy ordinary files only; never follow links out of an imported project."""
    source = source.resolve()
    if target.resolve().is_relative_to(source):
        raise BuildError("The output workspace must be outside the imported source folder.")
    for p in source.rglob("*"):
        if p.is_symlink() or not p.resolve().is_relative_to(source):
            raise BuildError(f"Linked files are not supported in imported projects: {p}")
        if p.is_file():
            dest = inside(target, p.relative_to(source).as_posix())
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, dest)


def atomic_copy(source: Path, target: Path, check_cancel=lambda: None):
    """Copy a large generated archive using a bounded buffer and atomic publish."""
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".builder-", dir=target.parent)
    try:
        with os.fdopen(fd, "wb") as out, source.open("rb") as data:
            while chunk := data.read(1024 * 1024):
                check_cancel()
                out.write(chunk)
            out.flush()
            os.fsync(out.fileno())
        check_cancel()
        if digest(Path(name)) != digest(source):
            raise BuildError("Staged copy did not match the source archive.")
        os.replace(name, target)
    finally:
        Path(name).unlink(missing_ok=True)
