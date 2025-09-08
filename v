#! /usr/bin/env python3
"""Quick theme switcher for VS Code."""

import json
from pathlib import Path

settings = Path.home() / "Library" / "Application Support" / "Code" / "User" / "settings.json"
vscode = json.loads(settings.read_text())

old = vscode["workbench.colorTheme"]
new = "Catppuccin Latte" if old == "Catppuccin Frappé" else "Catppuccin Frappé"

vscode["workbench.colorTheme"] = new
settings.write_text(json.dumps(vscode, ensure_ascii=False, indent=2) + "\n")

print(f"Switched from {old} to {new}")
