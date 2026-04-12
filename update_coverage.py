#!/usr/bin/env python3
"""Update the coverage table in README.md from cargo llvm-cov output."""

import re
import subprocess
import sys

README = "README.md"
START_TAG = "<!-- COVERAGE_START -->"
END_TAG = "<!-- COVERAGE_END -->"


def run_coverage() -> str:
    print("Running cargo llvm-cov...")
    result = subprocess.run(
        ["cargo", "llvm-cov", "--all-features", "--workspace"],
        capture_output=True,
        text=True,
    )
    return result.stdout + result.stderr


def extract_table(output: str) -> str:
    lines = output.splitlines()
    start = next((i for i, l in enumerate(lines) if l.startswith("Filename")), None)
    if start is None:
        raise ValueError("Could not find 'Filename' header in coverage output.")
    end = next((i for i in range(start, len(lines)) if lines[i].startswith("TOTAL")), None)
    if end is None:
        raise ValueError("Could not find 'TOTAL' line in coverage output.")
    return "\n".join(lines[start : end + 1])


def update_readme(table: str) -> None:
    new_block = f"{START_TAG}\n```text\n{table}\n```\n{END_TAG}"
    content = open(README, encoding="utf-8").read()
    pattern = r"<!-- COVERAGE_START -->.*?<!-- COVERAGE_END -->"
    updated = re.sub(pattern, new_block, content, flags=re.DOTALL)
    open(README, "w", encoding="utf-8").write(updated)


def main() -> None:
    try:
        output = run_coverage()
        table = extract_table(output)
        update_readme(table)
        print(f"Successfully updated {README} with the latest coverage report!")
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
