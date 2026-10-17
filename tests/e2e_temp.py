"""Shared scratch-root selection for subprocess E2E tests."""
import os
import stat
from pathlib import Path


_FILE_ATTRIBUTE_REPARSE_POINT = 0x400


def is_reparse_component(path: Path) -> bool:
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return False
    return stat.S_ISLNK(metadata.st_mode) or bool(
        getattr(metadata, "st_file_attributes", 0) & _FILE_ATTRIBUTE_REPARSE_POINT
    )


def assert_no_reparse_components(path: Path) -> None:
    """Reject symlinks and Windows reparse points along an output path."""
    absolute = Path(os.path.abspath(path))
    for component in reversed((absolute, *absolute.parents)):
        if is_reparse_component(component):
            raise ValueError(f"output path contains a linked component: {component}")


def e2e_temp_root(default: Path) -> Path:
    configured = os.environ.get("TOKITO_E2E_TEMP_ROOT")
    root = Path(configured).expanduser() if configured else default
    if not root.is_absolute():
        raise ValueError("TOKITO_E2E_TEMP_ROOT must be an absolute path")
    assert_no_reparse_components(root)
    root.mkdir(parents=True, exist_ok=True)
    assert_no_reparse_components(root)
    resolved = root.resolve(strict=True)
    if not resolved.is_dir():
        raise ValueError("TOKITO_E2E_TEMP_ROOT must be a directory")
    return resolved
