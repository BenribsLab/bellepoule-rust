#!/bin/sh
# Lance @PROGRAM@ (dossier bin/), d'où que l'on parte. Si le système ne
# convient pas (autre architecture, bibliothèques manquantes), le dit.
here="$(dirname "$(readlink -f "$0")")"
program="$here/bin/@PROGRAM@"

fail() {
    echo "$1" >&2
    if [ -t 1 ]; then
        :
    elif command -v zenity >/dev/null; then
        zenity --error --title "ManageComp" --text "$1"
    elif command -v kdialog >/dev/null; then
        kdialog --title "ManageComp" --error "$1"
    elif command -v notify-send >/dev/null; then
        notify-send "ManageComp" "$1"
    fi
    exit 1
}

# Architecture du système (celle des programmes, pas seulement du noyau :
# un Raspberry Pi OS 32 bits tourne souvent sur un noyau 64 bits).
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
    fail "Cette version de ManageComp est faite pour @ARCHLABEL@, mais ce système est « $arch ».

@ARCHHINT@"
fi

missing="$(ldd "$program" 2>/dev/null | awk '/not found/ { print $1 }')"
if [ -n "$missing" ]; then
    fail "Il manque des bibliothèques pour démarrer ManageComp :
$missing

Ouvrez un terminal dans le dossier de ManageComp et lancez : ./installer.sh"
fi
exec "$program" "$@"
