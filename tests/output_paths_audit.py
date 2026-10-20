"""Audit generated verification roots for reparse points and escapes."""
import argparse
import os
from pathlib import Path

from e2e_temp import assert_no_reparse_components, is_reparse_component


def audit(base: Path, output_paths: list[Path]) -> None:
    if not base.is_absolute():
        raise ValueError("verification base must be absolute")
    assert_no_reparse_components(base)
    canonical_base = base.resolve(strict=True)
    for output_path in output_paths:
        if not output_path.is_absolute():
            raise ValueError(f"output path must be absolute: {output_path}")
        assert_no_reparse_components(output_path)
        if not output_path.is_dir():
            raise ValueError(f"output path is missing or not a directory: {output_path}")
        canonical_output = output_path.resolve(strict=True)
        try:
            canonical_output.relative_to(canonical_base)
        except ValueError as error:
            raise ValueError(f"output path escaped verification base: {output_path}") from error

        def fail_walk(error: OSError) -> None:
            raise error

        for current, directory_names, file_names in os.walk(
            canonical_output, followlinks=False, onerror=fail_walk
        ):
            current_path = Path(current)
            for name in (*directory_names, *file_names):
                child = current_path / name
                if is_reparse_component(child):
                    raise ValueError(f"output tree contains a linked component: {child}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--path", type=Path, action="append", required=True)
    arguments = parser.parse_args()
    audit(arguments.base, arguments.path)
    print(f"PASS: audited {len(arguments.path)} output trees for reparse points and escapes")


if __name__ == "__main__":
    main()
