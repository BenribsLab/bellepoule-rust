#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Fabrique l'archive Windows d'une version de ManageComp.

    python tools/release.py v5.7.0-alpha.4

Résultat : dist/ManageComp-<version>-windows-x64.zip, qui contient

    ManageComp.exe                  lanceurs (icône ManageComp) : rien à
    ManageComp Organisation.exe     chercher dans bin/
    ManageComp Live.exe
    LISEZMOI.txt, LICENSE, CREDITS.md
    bin/    programmes et bibliothèques GTK nécessaires (trouvées en lisant
            les imports des exécutables, de proche en proche)
    lib/    chargeur d'images SVG de GTK
    share/  ressources de ManageComp, exemples, icônes et schémas GTK

GTK est pris dans GTK4_DIR (C:\\Users\\ben\\gtk4 par défaut).
"""

import os
import shutil
import struct
import subprocess
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))
GTK = os.environ.get("GTK4_DIR", r"C:\Users\ben\gtk4")
TARGET = os.path.join(ROOT, "target", "release-pack")
PROGRAMS = ["managecomp.exe", "managecomp-organisation.exe", "managecomp-live.exe"]
LAUNCHERS = ["ManageComp.exe", "ManageComp Organisation.exe", "ManageComp Live.exe"]

README = """ManageComp {version} — logiciel libre de gestion de compétitions d'escrime
==========================================================================

Démarrer :
  - ManageComp.exe                 le logiciel (double-clic) ;
  - ManageComp Organisation.exe    pistes, arbitres et planning ;
  - ManageComp Live.exe            écran de piste.

Astuce : clic droit sur ManageComp.exe > « Afficher plus d'options » >
« Envoyer vers » > « Bureau (créer un raccourci) » pour l'avoir sur le bureau.

Rien d'autre à installer : GTK est fourni dans le dossier bin.
Documentation : https://managecomp.org/docs/

ManageComp est un logiciel libre (GNU GPL version 3 ou ultérieure), dérivé de
BellePoule © 2009-2013 Yannick Le Roux. Voir LICENSE et CREDITS.md.
"""


def run(cmd, **kw):
    print("+", " ".join(cmd))
    subprocess.run(cmd, check=True, cwd=ROOT, **kw)


def pe_imports(path):
    """Noms des DLL importées par un exécutable Windows (format PE)."""
    with open(path, "rb") as f:
        data = f.read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    opt_size = struct.unpack_from("<H", data, pe + 20)[0]
    opt = pe + 24
    magic = struct.unpack_from("<H", data, opt)[0]
    dirs = opt + (112 if magic == 0x20B else 96)
    import_rva = struct.unpack_from("<I", data, dirs + 8)[0]
    table = opt + opt_size
    spans = []
    for i in range(sections):
        s = table + 40 * i
        vsize, va, rsize, raw = struct.unpack_from("<IIII", data, s + 8)
        spans.append((va, max(vsize, rsize), raw))

    def offset(rva):
        for va, size, raw in spans:
            if va <= rva < va + size:
                return rva - va + raw
        return None

    names = []
    at = offset(import_rva) if import_rva else None
    while at is not None:
        name_rva = struct.unpack_from("<I", data, at + 12)[0]
        if name_rva == 0:
            break
        o = offset(name_rva)
        end = data.index(b"\0", o)
        names.append(data[o:end].decode("ascii", "replace"))
        at += 20
    return names


def gtk_dependencies(binaries):
    """DLL de GTK nécessaires, de proche en proche (les DLL système restent)."""
    gtk_bin = os.path.join(GTK, "bin")
    available = {n.lower(): n for n in os.listdir(gtk_bin) if n.lower().endswith(".dll")}
    found, todo = {}, list(binaries)
    while todo:
        for dll in pe_imports(todo.pop()):
            key = dll.lower()
            if key in available and key not in found:
                found[key] = os.path.join(gtk_bin, available[key])
                todo.append(found[key])
    return sorted(found.values())


def copy_tree(src, dst, ignore=None):
    if os.path.isdir(src):
        shutil.copytree(src, dst, ignore=ignore, dirs_exist_ok=True)
    else:
        print(f"  (absent : {src})")


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    version = sys.argv[1]
    env = dict(os.environ, CARGO_TARGET_DIR=TARGET)
    env["PATH"] = os.path.join(GTK, "bin") + os.pathsep + env.get("PATH", "")
    run(["cargo", "build", "--release", "-p", "bellepoule", "-p", "marshaller", "--features", "marshaller/gui",
         "-p", "livepoule", "-p", "launcher"], env=env)
    built = os.path.join(TARGET, "release")

    name = f"ManageComp-{version}-windows-x64"
    dist = os.path.join(ROOT, "dist")
    out = os.path.join(dist, name)
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(os.path.join(out, "bin"))

    # Programmes et bibliothèques
    programs = []
    for p in PROGRAMS:
        programs.append(shutil.copy2(os.path.join(built, p), os.path.join(out, "bin", p)))
    loader = os.path.join(GTK, "lib", "gdk-pixbuf-2.0", "2.10.0")
    loader_dll = os.path.join(loader, "loaders", "pixbufloader_svg.dll")
    for dll in gtk_dependencies(programs + [loader_dll]):
        shutil.copy2(dll, os.path.join(out, "bin"))
    os.makedirs(os.path.join(out, "lib", "gdk-pixbuf-2.0", "2.10.0", "loaders"))
    shutil.copy2(loader_dll, os.path.join(out, "lib", "gdk-pixbuf-2.0", "2.10.0", "loaders"))
    shutil.copy2(os.path.join(loader, "loaders.cache"), os.path.join(out, "lib", "gdk-pixbuf-2.0", "2.10.0"))

    # Ressources
    share = os.path.join(out, "share")
    copy_tree(os.path.join(ROOT, "resources"), os.path.join(share, "bellepoule", "resources"),
              ignore=shutil.ignore_patterns("source"))
    copy_tree(os.path.join(ROOT, "original", "Exemples"), os.path.join(share, "bellepoule", "Exemples"))
    for theme in ("Adwaita", "hicolor"):
        copy_tree(os.path.join(GTK, "share", "icons", theme), os.path.join(share, "icons", theme))
    os.makedirs(os.path.join(share, "glib-2.0", "schemas"))
    shutil.copy2(os.path.join(GTK, "share", "glib-2.0", "schemas", "gschemas.compiled"), os.path.join(share, "glib-2.0", "schemas"))

    # Racine : lanceurs avec l'icône, notice et licence
    for launcher in LAUNCHERS:
        shutil.copy2(os.path.join(built, "managecomp-launcher.exe"), os.path.join(out, launcher))
    with open(os.path.join(out, "LISEZMOI.txt"), "w", encoding="utf-8-sig", newline="\r\n") as f:
        f.write(README.format(version=version))
    for doc in ("LICENSE", "CREDITS.md"):
        shutil.copy2(os.path.join(ROOT, doc), out)

    archive = os.path.join(dist, name + ".zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for folder, _, files in os.walk(out):
            for file in files:
                path = os.path.join(folder, file)
                z.write(path, os.path.relpath(path, dist))
    size = os.path.getsize(archive) / 1e6
    print(f"\n{archive} ({size:.1f} Mo)")


if __name__ == "__main__":
    main()
