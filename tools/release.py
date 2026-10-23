#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Fabrique les archives d'une version de ManageComp.

    python tools/release.py v5.7.0-alpha.4              Windows, Linux PC, ARM 64 et ARM 32 bits
    python tools/release.py v5.7.0-alpha.4 --windows    Windows (options cumulables)
    python tools/release.py v5.7.0-alpha.4 --linux      Linux PC (x86-64)
    python tools/release.py v5.7.0-alpha.4 --arm        Linux ARM 64 bits (Raspberry Pi 4 et 5)
    python tools/release.py v5.7.0-alpha.4 --armhf      Linux ARM 32 bits (Raspberry Pi OS 32 bits)

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

Linux : dist/ManageComp-<version>-linux-x64.tar.gz (PC), -linux-arm64.tar.gz et
-linux-armhf.tar.gz (Raspberry Pi 64 et 32 bits), compilés dans Docker
(tools/linux/Dockerfile, Dockerfile.arm64, Dockerfile.armhf : Ubuntu 24.04) ; GTK 4.12 ou plus récent est pris sur le système de l'utilisateur.
À la racine : `installer.sh` (vérifie et propose d'installer les bibliothèques,
ajoute les raccourcis avec le logo) et les lanceurs `ManageComp`,
`ManageComp-Organisation`, `ManageComp-Live` (tools/linux/launcher.sh).
"""

import os
import shutil
import io
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

Installation : ouvrez un terminal dans ce dossier et lancez

    ./installer.sh

Il vérifie les bibliothèques nécessaires (GTK 4.12 ou plus récent), propose
de les installer s'il en manque, puis ajoute ManageComp au menu des
applications (et sur le bureau si vous le souhaitez), avec son logo.

Systèmes : Ubuntu 24.04 et suivantes, Debian 13, Fedora 39 et suivantes,
Raspberry Pi OS « Trixie » (archive ARM 64 bits ou 32 bits selon le système).

Démarrer ensuite depuis le menu des applications, ou :
  - ./ManageComp                 le logiciel ;
  - ./ManageComp-Organisation    pistes, arbitres et planning ;
  - ./ManageComp-Live            écran de piste.

Documentation : https://managecomp.org/docs/

ManageComp est un logiciel libre (GNU GPL version 3 ou ultérieure), dérivé de
BellePoule © 2009-2013 Yannick Le Roux. Voir LICENSE et CREDITS.md.
"""

# Architecture → (image Docker, Dockerfile, cible Rust, options en plus).
LINUX_TARGETS = {
    "x64": ("managecomp-linux-build", "Dockerfile", None, []),
    # Raspberry Pi : GPIO de ManageComp Live (sans effet sans carte).
    "arm64": ("managecomp-linux-arm64-build", "Dockerfile.arm64", "aarch64-unknown-linux-gnu", ["--features", "livepoule/raspberry"]),
    "armhf": ("managecomp-linux-armhf-build", "Dockerfile.armhf", "armv7-unknown-linux-gnueabihf", ["--features", "livepoule/raspberry"]),
}


# Textes des scripts Linux quand le système n'a pas la bonne architecture.
ARCH_TEXTS = {
    "x64": {
        "@ARCH@": "amd64",
        "@ARCHLABEL@": "les PC Linux 64 bits (x86-64)",
        "@ARCHHINT@": "Téléchargez l'archive qui correspond à votre système : https://github.com/BenribsLab/managecomp-rust/releases (linux-arm64 ou linux-armhf pour un Raspberry Pi).",
    },
    "arm64": {
        "@ARCH@": "arm64",
        "@ARCHLABEL@": "les systèmes ARM 64 bits (Raspberry Pi OS 64 bits, Ubuntu pour Raspberry Pi)",
        "@ARCHHINT@": "Pour un système 32 bits (armhf), prenez l'archive linux-armhf : https://github.com/BenribsLab/managecomp-rust/releases",
    },
    "armhf": {
        "@ARCH@": "armhf",
        "@ARCHLABEL@": "les systèmes ARM 32 bits (Raspberry Pi OS 32 bits)",
        "@ARCHHINT@": "Pour un système 64 bits, prenez l'archive linux-arm64 (Raspberry Pi) ou linux-x64 (PC) : https://github.com/BenribsLab/managecomp-rust/releases",
    },
}


def build_linux(version, arch):
    """Compilation dans Docker, puis archive .tar.gz (droits d'exécution compris)."""
    image, dockerfile, target, extra = LINUX_TARGETS[arch]
    out_bin = os.path.join(ROOT, "dist", f"linux-{arch}-bin")
    os.makedirs(out_bin, exist_ok=True)
    linux = os.path.join(ROOT, "tools", "linux")
    run(["docker", "build", "-q", "-t", image, "-f", os.path.join(linux, dockerfile), linux])
    programs = ["managecomp", "managecomp-organisation", "managecomp-live"]
    release = f"/target/{target}/release" if target else "/target/release"
    copy = " ".join(f"{release}/{p}" for p in programs)
    cargo = ["cargo", "build", "--release", "-p", "bellepoule", "-p", "marshaller", "--features", "marshaller/gui", "-p", "livepoule"] + extra
    run(["docker", "run", "--rm",
         "-v", f"{ROOT}:/src", "-v", f"{out_bin}:/out",
         "-v", f"managecomp-linux-{arch}-target:/target", "-v", "managecomp-linux-cargo:/root/.cargo/registry",
         "-e", "CARGO_TARGET_DIR=/target", image,
         "sh", "-c", " ".join(cargo) + f" && cp {copy} /out/"],
        env=dict(os.environ, MSYS_NO_PATHCONV="1"))

    name = f"ManageComp-{version}-linux-{arch}"
    archive = os.path.join(ROOT, "dist", name + ".tar.gz")

    def add_bytes(tar, path, data, mode):
        info = tarfile.TarInfo(f"{name}/{path}")
        info.size, info.mode = len(data), mode
        tar.addfile(info, io.BytesIO(data))

    def add_tree(tar, src, dst, skip=()):
        for folder, dirs, files in os.walk(src):
            dirs[:] = [d for d in dirs if d not in skip]
            for f in files:
                path = os.path.join(folder, f)
                rel = os.path.relpath(path, src).replace(os.sep, "/")
                with open(path, "rb") as fh:
                    add_bytes(tar, f"{dst}/{rel}", fh.read(), 0o644)

    def script(file):
        with open(os.path.join(linux, file), encoding="utf-8") as fh:
            text = fh.read().replace("\r\n", "\n")
        for key, value in ARCH_TEXTS[arch].items():
            text = text.replace(key, value)
        return text

    with tarfile.open(archive, "w:gz", compresslevel=9) as tar:
        for p in programs:
            with open(os.path.join(out_bin, p), "rb") as fh:
                add_bytes(tar, f"bin/{p}", fh.read(), 0o755)
        add_tree(tar, os.path.join(ROOT, "resources"), "share/bellepoule/resources", skip=("source",))
        if os.path.isdir(os.path.join(ROOT, "original", "Exemples")):
            add_tree(tar, os.path.join(ROOT, "original", "Exemples"), "share/bellepoule/Exemples")
        for launcher, program in (("ManageComp", "managecomp"), ("ManageComp-Organisation", "managecomp-organisation"), ("ManageComp-Live", "managecomp-live")):
            add_bytes(tar, launcher, script("launcher.sh").replace("@PROGRAM@", program).encode(), 0o755)
        add_bytes(tar, "installer.sh", script("installer.sh").encode(), 0o755)
        add_bytes(tar, "LISEZMOI.txt", README_LINUX.format(version=version).encode(), 0o644)
        for doc in ("LICENSE", "CREDITS.md"):
            with open(os.path.join(ROOT, doc), "rb") as fh:
                add_bytes(tar, doc, fh.read(), 0o644)
    print(f"\n{archive} ({os.path.getsize(archive) / 1e6:.1f} Mo)")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = {a for a in sys.argv[1:] if a.startswith("--")}
    if len(args) != 1 or not flags <= {"--windows", "--linux", "--arm", "--armhf"}:
        sys.exit(__doc__)
    version = args[0]
    every = not flags
    if every or "--windows" in flags:
        build_windows(version)
    if every or "--linux" in flags:
        build_linux(version, "x64")
    if every or "--arm" in flags:
        build_linux(version, "arm64")
    if every or "--armhf" in flags:
        build_linux(version, "armhf")


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
