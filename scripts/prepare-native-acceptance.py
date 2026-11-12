#!/usr/bin/env python3
"""Create and retain synthetic native-acceptance fixtures; never launch anything."""
import os
from pathlib import Path
import secrets
import sys

if sys.platform != "darwin":
    raise SystemExit("Native acceptance fixtures currently support macOS only")

root = Path("/private/tmp") / ("figcon-native-" + secrets.token_hex(16))
root.mkdir(mode=0o700)
# No cleanup: fixtures, staging and evidence must remain available for inspection.
for name in ("home", "home/.codex", "data", "bin"):
    (root / name).mkdir(mode=0o700)


def private_file(relative, text, mode=0o600):
    # Exclusive creation avoids replacing anything, including through a link.
    fd = os.open(root / relative, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        stream.write(text)


private_file(".figcon-native-acceptance", "figcon-native-acceptance-v1\n")
private_file("home/.claude.json", '{"syntheticAcceptanceSetting":"not-a-secret"}\n')
private_file("home/.codex/config.toml", '# Synthetic acceptance settings\nsynthetic_acceptance_setting = "not-a-secret"\n')
for name in ("claude", "codex"):
    private_file("bin/" + name, "Synthetic detection fixture. This file must never execute.\n", 0o700)
print(root)
