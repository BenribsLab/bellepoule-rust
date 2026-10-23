#!/bin/sh
# Installation de ManageComp sous Linux (pour l'utilisateur courant).
#
#   ./installer.sh                 vérifie les bibliothèques (et propose de
#                                  les installer), puis ajoute les raccourcis
#   ./installer.sh --desinstaller  retire les raccourcis
#
# Le logiciel reste dans ce dossier : rien n'est copié ailleurs.

here="$(dirname "$(readlink -f "$0")")"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
desktop="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"
programs="managecomp managecomp-organisation managecomp-live"

ask() { # question, réponse par défaut O
    printf '%s [O/n] ' "$1"
    read -r answer || answer=n
    case "$answer" in n* | N*) return 1 ;; *) return 0 ;; esac
}

if [ "$1" = "--desinstaller" ]; then
    for p in $programs; do rm -f "$apps/$p.desktop" "$desktop/$p.desktop"; done
    echo "Raccourcis de ManageComp retirés."
    exit 0
fi

echo "=== Installation de ManageComp ==="
echo

# --- Architecture --------------------------------------------------------
# Celle des programmes du système, pas seulement du noyau (un Raspberry Pi
# OS 32 bits tourne souvent sur un noyau 64 bits).
arch="$(dpkg --print-architecture 2>/dev/null)"
if [ -z "$arch" ]; then
    case "$(uname -m)-$(getconf LONG_BIT 2>/dev/null)" in
        x86_64-64) arch=amd64 ;;
        aarch64-64 | arm64-64) arch=arm64 ;;
        armv7l-32 | armv8l-32 | armv6l-32) arch=armhf ;;
        *) arch="$(uname -m) $(getconf LONG_BIT 2>/dev/null) bits" ;;
    esac
fi
if [ "$arch" != "@ARCH@" ]; then
    echo "Cette version de ManageComp est faite pour @ARCHLABEL@,"
    echo "mais ce système est « $arch »."
    echo
    echo "@ARCHHINT@"
    exit 1
fi

# --- Bibliothèques -------------------------------------------------------
missing() { ldd "$here/bin/managecomp" 2>/dev/null | awk '/not found/ { print $1 }'; }

gtk_minor() { # 14 pour GTK 4.14 ; vide si GTK 4 est absent
    lib="$(ldconfig -p 2>/dev/null | awk '/libgtk-4\.so\.1 / { print $NF; exit }')"
    [ -n "$lib" ] || return 0
    real="$(readlink -f "$lib")"
    v="${real##*libgtk-4.so.1.}"
    echo "$((${v%%.*} / 100))"
}

packages() { # gestionnaire de paquets et paquets à installer
    if command -v apt-get >/dev/null; then echo "apt-get install -y libgtk-4-1 libudev1"
    elif command -v dnf >/dev/null; then echo "dnf install -y gtk4 systemd-libs"
    elif command -v zypper >/dev/null; then echo "zypper install -y libgtk-4-1 libudev1"
    elif command -v pacman >/dev/null; then echo "pacman -S --needed --noconfirm gtk4 systemd-libs"
    fi
}

need="$(missing)"
if [ -n "$need" ]; then
    echo "Bibliothèques manquantes :"
    echo "$need" | sed 's/^/  - /'
    echo
    cmd="$(packages)"
    if [ -z "$cmd" ]; then
        echo "Installez GTK 4 (4.12 ou plus récent) avec le gestionnaire de paquets de votre système, puis relancez ./installer.sh."
        exit 1
    fi
    if ask "Les installer maintenant (sudo $cmd) ?"; then
        # Le gestionnaire de paquets ne doit pas lire les réponses suivantes.
        [ "$(command -v apt-get)" ] && sudo apt-get update </dev/null
        # shellcheck disable=SC2086
        sudo $cmd </dev/null || { echo "L'installation a échoué."; exit 1; }
    else
        echo "Installation annulée : ManageComp ne pourra pas démarrer sans ces bibliothèques."
        exit 1
    fi
    need="$(missing)"
    if [ -n "$need" ]; then
        echo "Il manque toujours :"
        echo "$need" | sed 's/^/  - /'
        exit 1
    fi
fi

minor="$(gtk_minor)"
if [ -n "$minor" ] && [ "$minor" -lt 12 ]; then
    echo "Votre système fournit GTK 4.$minor ; ManageComp demande GTK 4.12 ou plus récent"
    echo "(Ubuntu 24.04 et suivantes, Debian 13, Fedora 39 et suivantes, Raspberry Pi OS « Trixie »)."
    echo "Mettez le système à jour, puis relancez ./installer.sh."
    exit 1
fi
echo "Bibliothèques : OK${minor:+ (GTK 4.$minor)}"
echo

# --- Raccourcis ----------------------------------------------------------
entry() { # fichier, nom, programme
    cat > "$1" <<EOF
[Desktop Entry]
Type=Application
Name=$2
Comment=Compétitions d'escrime
Exec="$here/bin/$3" %f
Icon=$here/share/bellepoule/resources/branding/logo.png
Terminal=false
Categories=Office;Sports;
EOF
    chmod +x "$1"
}
names() {
    case "$1" in
        managecomp) echo "ManageComp" ;;
        managecomp-organisation) echo "ManageComp Organisation" ;;
        managecomp-live) echo "ManageComp Live" ;;
    esac
}

if ask "Ajouter ManageComp au menu des applications ?"; then
    mkdir -p "$apps"
    for p in $programs; do entry "$apps/$p.desktop" "$(names "$p")" "$p"; done
    command -v update-desktop-database >/dev/null && update-desktop-database "$apps" 2>/dev/null
    echo "  Ajouté au menu des applications."
fi
if [ -d "$desktop" ] && ask "Ajouter une icône ManageComp sur le bureau ?"; then
    entry "$desktop/managecomp.desktop" "ManageComp" managecomp
    command -v gio >/dev/null && gio set "$desktop/managecomp.desktop" metadata::trusted true 2>/dev/null
    echo "  Icône ajoutée sur le bureau."
fi

echo
echo "Installation terminée. Pour retirer les raccourcis : ./installer.sh --desinstaller"
if ask "Lancer ManageComp maintenant ?"; then
    nohup "$here/bin/managecomp" >/dev/null 2>&1 &
fi
