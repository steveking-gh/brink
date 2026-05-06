#!/usr/bin/env python3
"""Renumber one error-code prefix into the ERR_nnn namespace.

Usage:
    rename_err_prefix.py [--dry-run] <PREFIX> <START_NUMBER>

Options:
    --dry-run   Show each line that would change; do not write files.

Example:
    rename_err_prefix.py AST 15
    rename_err_prefix.py --dry-run AST 15

Uses 'git ls-files' to enumerate source files, so build artifacts and
anything in .gitignore are automatically excluded.

Finds every occurrence of PREFIX_<digits>, shows them with file path and
line number, prompts for confirmation, then replaces each unique code with
ERR_<n> where n starts at START_NUMBER and increments by 1.  Unique codes
are assigned in ascending order of their original numeric suffix.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path


def iter_files():
    """Yield Path objects for every file known to git."""
    result = subprocess.run(
        ['git', 'ls-files'],
        capture_output=True, text=True, check=True,
    )
    for line in result.stdout.splitlines():
        path = Path(line)
        if path.is_file():
            yield path


def find_matches(prefix: str) -> list[tuple[str, int, str, str]]:
    """Return (filepath, line_num, line_text, code) for every match."""
    pattern = re.compile(r'\b' + re.escape(prefix) + r'_\d+')
    results = []
    for path in iter_files():
        try:
            text = path.read_text(encoding='utf-8')
        except (UnicodeDecodeError, PermissionError):
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            for m in pattern.finditer(line):
                results.append((str(path), lineno, line.rstrip(), m.group()))
    return results


def build_replacement_map(matches: list, start: int) -> dict[str, str]:
    """Map each unique found code to ERR_<n>, sorted by original numeric suffix."""
    unique = sorted(
        {code for _, _, _, code in matches},
        key=lambda c: int(c.split('_', 1)[1]),
    )
    return {old: f'ERR_{start + i}' for i, old in enumerate(unique)}


def make_combined_pattern(replacement_map: dict[str, str]) -> re.Pattern:
    return re.compile(
        '|'.join(r'\b' + re.escape(k) + r'\b' for k in replacement_map)
    )


def dry_run(matches: list, replacement_map: dict[str, str]) -> None:
    """Print every line that would change, with before/after content."""
    combined = make_combined_pattern(replacement_map)
    files_seen: dict[str, list[tuple[int, str, str]]] = {}
    for filepath in sorted({fp for fp, _, _, _ in matches}):
        text = Path(filepath).read_text(encoding='utf-8')
        changes = []
        for lineno, line in enumerate(text.splitlines(), 1):
            new_line = combined.sub(lambda m: replacement_map[m.group()], line)
            if new_line != line:
                changes.append((lineno, line.rstrip(), new_line.rstrip()))
        if changes:
            files_seen[filepath] = changes

    for filepath, changes in files_seen.items():
        print(f'{filepath}:')
        for lineno, old, new in changes:
            print(f'  {lineno}-  {old}')
            print(f'  {lineno}+  {new}')
        print()


def apply_replacements(matches: list, replacement_map: dict[str, str]) -> None:
    """Rewrite every file that contains at least one match."""
    combined = make_combined_pattern(replacement_map)
    for filepath in sorted({fp for fp, _, _, _ in matches}):
        text = Path(filepath).read_text(encoding='utf-8')
        new_text = combined.sub(lambda m: replacement_map[m.group()], text)
        Path(filepath).write_text(new_text, encoding='utf-8')


def main() -> None:
    parser = argparse.ArgumentParser(
        description='Rename one error-code prefix to ERR_nnn.',
        add_help=True,
    )
    parser.add_argument('--dry-run', action='store_true',
                        help='Show changes without writing files.')
    parser.add_argument('prefix', help='Error code prefix, e.g. AST')
    parser.add_argument('start', type=int, help='First ERR_<n> number to assign')
    args = parser.parse_args()

    matches = find_matches(args.prefix)
    if not matches:
        print(f"No instances of '{args.prefix}_<N>' found.")
        return

    print(f"Found {len(matches)} instance(s) of '{args.prefix}_<N>':\n")
    for filepath, lineno, line, code in matches:
        snippet = line.strip()
        if len(snippet) > 100:
            snippet = snippet[:97] + '...'
        print(f'  {filepath}:{lineno}  [{code}]  {snippet}')

    replacement_map = build_replacement_map(matches, args.start)

    print('\nReplacement mapping:')
    for old, new in replacement_map.items():
        print(f'  {old} -> {new}')

    if args.dry_run:
        print('\n--- dry run: line diffs ---\n')
        dry_run(matches, replacement_map)
        print('Dry run complete.  No files written.')
        return

    print()
    try:
        answer = input('convert (Y/n): ').strip().lower()
    except (EOFError, KeyboardInterrupt):
        print('\nAborted.')
        return

    if answer == 'n':
        print('Aborted.')
        return

    apply_replacements(matches, replacement_map)

    files_changed = len({fp for fp, _, _, _ in matches})
    print(f'\nDone: {len(replacement_map)} unique code(s) renamed across {files_changed} file(s).')


if __name__ == '__main__':
    main()
