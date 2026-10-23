#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Fabrique les archives d'une version de ManageComp.

    python tools/release.py v5.7.0-alpha.4              Windows et Linux
    python tools/release.py v5.7.0-alpha.4 --windows    Windows seulement
    python tools/release.py v5.7.0-alpha.4 --linux      Linux seulement

Windows : dist/ManageComp-<version>-windows-x64.zip, qui contient

    ManageComp.exe                  lanceurs (icône ManageComp) : rien à
    ManageComp Organisation.exe     chercher dans bin/
    ManageComp Live.exe
    LISEZMOI.txt, LICENSE, CREDITS.md
    bin/    programmes et bibliothèques GTK nécessaires (trouvées en lisant
            les imports des exécutables, de proche en proche)
    lib/    chargeur d'images SVG de GTK
    share/  ressources de ManageComp, exemples, icônes et schémas GTK

GTK est pris dans GTK4_DIR (C:\\Users\\ben\\gtk4 par défaut).

Linux : dist/ManageComp-<version>-linux-x64.tar.gz, compilé dans Docker
(tools/linux/Dockerfile, Ubuntu 24.04) ; GTK 4.12 ou plus récent est pris
sur le système de l'utilisateur. Lanceurs `ManageComp`, `ManageComp-Organisation`,
`ManageComp-Live` et `installer-raccourcis.sh` (menu des applications).
"""

import os
import shutil
import struct
import subprocess
import sys
import tarfile
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


README_LINUX = """ManageComp {version} — logiciel libre de gestion de compétitions d'escrime
==========================================================================

Prérequis : GTK 4.12 ou plus récent (Ubuntu 24.04 et suivantes, Debian 13,
Fedora 39 et suivantes). Sur Ubuntu ou Debian :  sudo apt install libgtk-4-1

Démarrer (double-clic ou depuis un terminal) :
  - ./ManageComp                 le logiciel ;
  - ./ManageComp-Organisation    pistes, arbitres et planning ;
  - ./ManageComp-Live            écran de piste.

Raccourcis avec le logo dans le menu des applications :
  ./installer-raccourcis.sh

Documentation : https://managecomp.org/docs/

ManageComp est un logiciel libre (GNU GPL version 3 ou ultérieure), dérivé de
BellePoule © 2009-2013 Yannick Le Roux. Voir LICENSE et CREDITS.md.
"""

LINUX_LAUNCHER = """#!/bin/sh
# Lance {program} (dossier bin/), d'où que l'on parte.
here="$(dirname "$(readlink -f "$0")")"
exec "$here/bin/{program}" "$@"
"""

LINUX_SHORTCUTS = """#!/bin/sh
# Ajoute ManageComp au menu des applications (avec son logo).
here="$(dirname "$(readlink -f "$0")")"
dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$dir"
icon="$here/share/bellepoule/resources/branding/logo.png"
make() {
    cat > "$dir/$1.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$2
Comment=Compétitions d'escrime
Exec="$here/bin/$3" %f
Icon=$icon
Terminal=false
Categories=Office;Sports;
EOF
}
make managecomp "ManageComp" managecomp
make managecomp-organisation "ManageComp Organisation" managecomp-organisation
make managecomp-live "ManageComp Live" managecomp-live
echo "Raccourcis ajoutés dans $dir"
"""


def build_linux(version):
    """Compilation dans Docker, puis archive .tar.gz (droits d'exécution compris)."""
    out_bin = os.path.join(ROOT, "dist", "linux-bin")
    os.makedirs(out_bin, exist_ok=True)
    run(["docker", "build", "-q", "-t", "managecomp-linux-build", os.path.join("tools", "linux")])
    programs = ["managecomp", "managecomp-organisation", "managecomp-live"]
    copy = " ".join(f"/target/release/{p}" for p in programs)
    run(["docker", "run", "--rm",
         "-v", f"{ROOT}:/src", "-v", f"{out_bin}:/out",
         "-v", "managecomp-linux-target:/target", "-v", "managecomp-linux-cargo:/root/.cargo/registry",
         "-e", "CARGO_TARGET_DIR=/target", "managecomp-linux-build",
         "sh", "-c", f"cargo build --release -p bellepoule -p marshaller --features marshaller/gui -p livepoule && cp {copy} /out/"],
        env=dict(os.environ, MSYS_NO_PATHCONV="1"))

    name = f"ManageComp-{version}-linux-x64"
    archive = os.path.join(ROOT, "dist", name + ".tar.gz")

    def add_text(tar, path, text, mode):
        data = text.encode("utf-8")
        info = tarfile.TarInfo(f"{name}/{path}")
        info.size, info.mode = len(data), mode
        import io
        tar.addfile(info, io.BytesIO(data))

    def add_tree(tar, src, dst, skip=()):
        for folder, dirs, files in os.walk(src):
            dirs[:] = [d for d in dirs if d not in skip]
            for f in files:
                path = os.path.join(folder, f)
                rel = os.path.relpath(path, src).replace(os.sep, "/")
                info = tar.gettarinfo(path, f"{name}/{dst}/{rel}")
                info.mode = 0o644
                with open(path, "rb") as fh:
                    tar.addfile(info, fh)

    with tarfile.open(archive, "w:gz", compresslevel=9) as tar:
        for p in programs:
            path = os.path.join(out_bin, p)
            info = tar.gettarinfo(path, f"{name}/bin/{p}")
            info.mode = 0o755
            with open(path, "rb") as fh:
                tar.addfile(info, fh)
        add_tree(tar, os.path.join(ROOT, "resources"), "share/bellepoule/resources", skip=("source",))
        if os.path.isdir(os.path.join(ROOT, "original", "Exemples")):
            add_tree(tar, os.path.join(ROOT, "original", "Exemples"), "share/bellepoule/Exemples")
        for launcher, program in (("ManageComp", "managecomp"), ("ManageComp-Organisation", "managecomp-organisation"), ("ManageComp-Live", "managecomp-live")):
            add_text(tar, launcher, LINUX_LAUNCHER.format(program=program), 0o755)
        add_text(tar, "installer-raccourcis.sh", LINUX_SHORTCUTS, 0o755)
        add_text(tar, "LISEZMOI.txt", README_LINUX.format(version=version), 0o644)
        for doc in ("LICENSE", "CREDITS.md"):
            with open(os.path.join(ROOT, doc), encoding="utf-8") as fh:
                add_text(tar, doc, fh.read(), 0o644)
    print(f"\n{archive} ({os.path.getsize(archive) / 1e6:.1f} Mo)")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = {a for a in sys.argv[1:] if a.startswith("--")}
    if len(args) != 1 or not flags <= {"--windows", "--linux"}:
        sys.exit(__doc__)
    version = args[0]
    both = not flags
    if both or "--windows" in flags:
        build_windows(version)
    if both or "--linux" in flags:
        build_linux(version)


def build_windows(version):
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
