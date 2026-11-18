#!/usr/bin/env python3
"""Report hardcoded user-facing strings in GUI widget calls."""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GUI = ROOT / "src" / "gui"
CALLS = (
    "ui.label(",
    "ui.button(",
    "ui.small_button(",
    "ui.heading(",
    "ui.checkbox(",
    "ui.selectable_label(",
    "Window::new(",
    "RichText::new(",
    "egui::Button::new(",
)
TECHNICAL = re.compile(r"^[^A-Za-z]*$")


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def literal_after(text: str, start: int) -> tuple[str, int] | None:
    cursor = start
    while cursor < len(text) and text[cursor].isspace():
        cursor += 1
    if cursor >= len(text) or text[cursor] != '"':
        return None
    while cursor < len(text):
        char = text[cursor]
        if char == '"':
            end = cursor + 1
            escaped = False
            while end < len(text):
                current = text[end]
                if current == '"' and not escaped:
                    return text[cursor + 1 : end], cursor
                escaped = current == "\\" and not escaped
                if current != "\\":
                    escaped = False
                end += 1
            return None
        if char in "),;\n":
            return None
        cursor += 1
    return None


def scan_file(path: Path) -> list[tuple[int, str]]:
    text = path.read_text(encoding="utf-8")
    test_start = text.find("#[cfg(test)]")
    if test_start >= 0:
        text = text[:test_start]
    violations: list[tuple[int, str]] = []
    seen: set[tuple[int, str]] = set()
    for call in CALLS:
        start = 0
        while (index := text.find(call, start)) >= 0:
            start = index + len(call)
            line_start = text.rfind("\n", 0, index) + 1
            line_end = text.find("\n", index)
            line = text[line_start : line_end if line_end >= 0 else len(text)]
            if "i18n-allow" in line or line.lstrip().startswith("//"):
                continue
            argument = start
            if call in ("ui.checkbox(", "ui.selectable_label("):
                comma = text.find(",", start)
                if comma < 0:
                    continue
                argument = comma + 1
            found = literal_after(text, argument)
            if not found:
                continue
            value, value_start = found
            if TECHNICAL.match(value) or not re.search(r"[A-Za-z]{2,}", value):
                continue
            line = line_number(text, value_start)
            key = (line, value)
            if key not in seen:
                seen.add(key)
                violations.append(key)
    return sorted(violations)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--strict",
        action="store_true",
        default=os.environ.get("I18N_AUDIT_STRICT") == "1",
        help="exit non-zero when hardcoded strings are found",
    )
    args = parser.parse_args()

    violations: list[tuple[Path, int, str]] = []
    for path in sorted(GUI.rglob("*.rs")):
        for line, value in scan_file(path):
            violations.append((path.relative_to(ROOT), line, value))

    if violations:
        print(f"i18n audit: {len(violations)} hardcoded UI string(s)")
        for path, line, value in violations:
            print(f"{path}:{line}: {value}")
    else:
        print("i18n audit: clean")

    if violations and args.strict:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
