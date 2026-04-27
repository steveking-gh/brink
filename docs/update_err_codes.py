#!/usr/bin/env python3
"""
update_err_codes.py -- Scan Brink Rust source for error/warning codes and
rewrite docs/error_codes.md with a sorted, de-duplicated inventory.

Two emission patterns are recognized:

  1. Bare "CODE" string literals in any .rs file (excluding target/).
     Catches diags.err*/warn*/note* calls, and codes passed to internal
     helpers such as err_expected_after, err_invalid_expression,
     coerce_numeric_pair, etc.  Test assertions use "[CODE]" (with
     brackets), so no test-file exclusion is needed for this pattern.

  2. "[CODE]" bracket-format strings in non-test source files.
     Catches pipeline-level halt codes (PROC prefix) that surface only in
     anyhow!() / .context() error chains rather than through diags.
     The top-level tests/ directory is excluded to avoid picking up
     integration-test assertion strings.

Usage:
    python3 docs/update_err_codes.py [--root PATH] [--dry-run]

Options:
    --root PATH   Project root.  Default: parent of the docs/ directory.
    --dry-run     Print the generated content instead of writing the file.
"""

import re
import sys
import argparse
from pathlib import Path
from collections import defaultdict

# Matches any bare error-code string literal: "PREFIX_DIGITS".
# Catches diags.err*/warn*/note* calls AND codes passed to internal helpers
# (err_expected_after, coerce_numeric_pair, etc.).  Test files use bracket
# format "[CODE]" for assertions, so no exclusion is needed.
BARE_RE = re.compile(r'"([A-Z]+_\d+)"')

# Matches bracket-format codes embedded in anyhow! or .context() strings.
# Example: return Err(anyhow!("[PROC_9]: Error detected, halting."));
#          .context("[PROC_3]: Error detected, halting.")?;
# Applied only to non-test source to avoid integration-test assertions.
BRACKET_RE = re.compile(r'\[([A-Z]+_\d+)\]')


def is_test_file(path: Path, root: Path) -> bool:
    """True for files under the top-level tests/ directory."""
    try:
        rel = path.relative_to(root)
    except ValueError:
        return False
    return rel.parts[0] == 'tests'


def scan_file(path: Path, root: Path, codes: set) -> None:
    try:
        text = path.read_text(encoding='utf-8', errors='replace')
    except OSError as e:
        print(f'Warning: cannot read {path}: {e}', file=sys.stderr)
        return

    # Pattern 1: bare "CODE" literals -- safe everywhere since test
    # assertions always use the bracketed form "[CODE]".
    for m in BARE_RE.finditer(text):
        codes.add(m.group(1))

    # Pattern 2: bracket-format codes in non-test source only.
    if not is_test_file(path, root):
        for m in BRACKET_RE.finditer(text):
            codes.add(m.group(1))


def scan_tree(root: Path) -> set:
    codes = set()
    for rs_file in root.rglob('*.rs'):
        if 'target' in rs_file.parts:
            continue
        scan_file(rs_file, root, codes)
    return codes


def group_and_sort(codes: set) -> dict:
    """Return {prefix: [n, n, ...]} with both sorted."""
    groups: dict[str, list[int]] = defaultdict(list)
    for code in codes:
        prefix, num = code.rsplit('_', 1)
        groups[prefix].append(int(num))
    return {prefix: sorted(groups[prefix]) for prefix in sorted(groups)}


def next_available(nums: list) -> int:
    """Next code number for a prefix: one past the current maximum."""
    return max(nums) + 1


def render(groups: dict) -> str:
    next_parts = ', '.join(
        f'{p}_{next_available(ns)}' for p, ns in groups.items()
    )
    lines = [
        '# Error Codes',
        '',
        'One entry per code in use, sorted numerically within each prefix.',
        f'Next available per prefix: {next_parts}.',
        '',
    ]
    for prefix, nums in groups.items():
        lines.append(f'## {prefix}')
        lines.append('')
        for n in nums:
            lines.append(f'{prefix}_{n}')
        lines.append('')
    # Strip trailing blank line, add final newline.
    return '\n'.join(lines).rstrip() + '\n'


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        '--root', default=None,
        help="Project root (default: parent of the docs/ directory)"
    )
    parser.add_argument(
        '--dry-run', action='store_true',
        help="Print generated content to stdout instead of writing the file"
    )
    args = parser.parse_args()

    script_path = Path(__file__).resolve()
    docs_dir = script_path.parent
    root = Path(args.root).resolve() if args.root else docs_dir.parent
    out_path = docs_dir / 'error_codes.md'

    print(f'Scanning {root} ...', file=sys.stderr)
    codes = scan_tree(root)

    if not codes:
        print('No error codes found -- check --root path.', file=sys.stderr)
        sys.exit(1)

    groups = group_and_sort(codes)
    content = render(groups)

    if args.dry_run:
        sys.stdout.write(content)
    else:
        out_path.write_text(content, encoding='utf-8')
        total = sum(len(v) for v in groups.values())
        print(f'Wrote {out_path}: {total} codes across {len(groups)} prefixes.')

    for prefix, nums in groups.items():
        gaps = sorted(set(range(1, max(nums) + 1)) - set(nums))
        if not gaps:
            gap_str = ''
        elif len(gaps) <= 8:
            gap_str = f'  gaps: {gaps}'
        else:
            gap_str = f'  {len(gaps)} gaps'
        print(
            f'  {prefix:10s} {len(nums):3d} codes  '
            f'next={prefix}_{next_available(nums)}{gap_str}',
            file=sys.stderr,
        )


if __name__ == '__main__':
    main()
