"""Unit checks for local E2E scratch-path refusal rules."""
import os
import stat
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from e2e_temp import assert_no_reparse_components, e2e_temp_root
from output_paths_audit import audit


class SyntheticMetadata:
    def __init__(self, mode: int, attributes: int = 0):
        self.st_mode = mode
        self.st_file_attributes = attributes


class E2ETempPathTests(unittest.TestCase):
    def setUp(self):
        # The full verifier intentionally sets this variable for its subprocesses.
        # These unit cases exercise default/invalid input independently of that
        # runner-level configuration.
        self.environment_patch = patch.dict(os.environ, {"TOKITO_E2E_TEMP_ROOT": ""})
        self.environment_patch.start()

    def tearDown(self):
        self.environment_patch.stop()

    def test_rejects_relative_configured_root(self):
        with self.assertRaisesRegex(ValueError, "absolute path"):
            e2e_temp_root(Path("relative-scratch"))

    def test_accepts_and_creates_absolute_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "scratch"
            self.assertEqual(e2e_temp_root(root), root.resolve(strict=True))

    def test_rejects_symbolic_link_component(self):
        linked = SyntheticMetadata(stat.S_IFLNK | 0o777)
        with patch.object(Path, "lstat", return_value=linked):
            with self.assertRaisesRegex(ValueError, "linked component"):
                assert_no_reparse_components(Path("C:/safe/scratch"))

    def test_rejects_windows_reparse_component(self):
        directory_with_junction_attribute = SyntheticMetadata(
            stat.S_IFDIR | 0o755, attributes=0x400
        )
        with patch.object(Path, "lstat", return_value=directory_with_junction_attribute):
            with self.assertRaisesRegex(ValueError, "linked component"):
                assert_no_reparse_components(Path("C:/safe/scratch"))

    def test_audit_rejects_output_outside_verification_base(self):
        with tempfile.TemporaryDirectory() as base, tempfile.TemporaryDirectory() as outside:
            with self.assertRaisesRegex(ValueError, "escaped verification base"):
                audit(Path(base), [Path(outside)])

    def test_audit_rejects_linked_descendant(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            output = base / "output"
            output.mkdir()
            linked_child = output / "junction-target"
            linked_child.mkdir()
            with patch(
                "output_paths_audit.is_reparse_component",
                side_effect=lambda path: path == linked_child,
            ):
                with self.assertRaisesRegex(ValueError, "linked component"):
                    audit(base, [output])


if __name__ == "__main__":
    unittest.main()
