"""Host regressions for the independent-reference bounds calculation."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

spec = importlib.util.spec_from_file_location(
    "evaluated_reference", Path(__file__).resolve().parents[1] / "scripts/evaluate-step-reference.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class EvaluatedBoundsTest(unittest.TestCase):
    def test_all_vertices_contribute(self):
        points = [SimpleNamespace(x=1, y=-2, z=3), SimpleNamespace(x=-4, y=5, z=-6),
                  SimpleNamespace(x=10, y=0, z=0)]
        self.assertEqual(module.evaluated_bounds(points),
                         {"min": [-4, -2, -6], "max": [10, 5, 3]})

    def test_empty_is_not_geometry(self):
        with self.assertRaises(ValueError):
            module.evaluated_bounds([])

    def test_non_first_nonfinite_vertex_is_rejected(self):
        for value in (float("nan"), float("inf"), -float("inf")):
            with self.subTest(value=value), self.assertRaises(ValueError):
                module.evaluated_bounds([SimpleNamespace(x=0, y=0, z=0),
                                         SimpleNamespace(x=0, y=value, z=0)])


if __name__ == "__main__":
    unittest.main()
