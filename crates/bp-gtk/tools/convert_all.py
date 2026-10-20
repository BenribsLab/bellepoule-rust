#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Convertit toutes les interfaces du superviseur (resources/glade -> resources/ui)
puis les valide avec gtk4-builder-tool (si disponible dans le PATH)."""

import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import glade_to_ui  # noqa: E402

FILES = ["tournament", "contest", "checkin", "form", "referees", "schedule", "pool",
         "pool_allocator", "pool_supervisor", "pillow_list", "table", "table_supervisor",
         "barrage", "splitting", "classification", "general_classification", "advertiser"]


def main():
    src_dir = os.path.join(ROOT, "resources", "glade")
    dst_dir = os.path.join(ROOT, "resources", "ui")
    os.makedirs(dst_dir, exist_ok=True)
    only = sys.argv[1:] or FILES
    failures = 0
    for name in only:
        src = os.path.join(src_dir, name + ".glade")
        dst = os.path.join(dst_dir, name + ".ui")
        sys.argv = ["glade_to_ui", src, dst, name.replace("_", "-")]
        glade_to_ui.main()
        try:
            r = subprocess.run(["gtk4-builder-tool", "validate", dst],
                               capture_output=True, text=True, encoding="utf-8")
            status = "OK" if r.returncode == 0 and not r.stderr.strip() else "ERREUR"
            if status != "OK":
                failures += 1
            print("%-25s %s %s" % (name, status, (r.stdout + r.stderr).strip()[:2000]))
        except FileNotFoundError:
            print("%-25s converti (gtk4-builder-tool absent)" % name)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
