#!/bin/sh
# Lance @PROGRAM@ (dossier bin/), d'où que l'on parte. S'il manque des
# bibliothèques, le dit et renvoie vers ./installer.sh.
here="$(dirname "$(readlink -f "$0")")"
program="$here/bin/@PROGRAM@"
missing="$(ldd "$program" 2>/dev/null | awk '/not found/ { print $1 }')"
if [ -n "$missing" ]; then
    message="Il manque des bibliothèques pour démarrer ManageComp :
$missing

Ouvrez un terminal dans le dossier de ManageComp et lancez : ./installer.sh"
    echo "$message" >&2
    if [ -t 1 ]; then
        :
    elif command -v zenity >/dev/null; then
        zenity --error --title "ManageComp" --text "$message"
    elif command -v kdialog >/dev/null; then
        kdialog --title "ManageComp" --error "$message"
    elif command -v notify-send >/dev/null; then
        notify-send "ManageComp" "$message"
    fi
    exit 1
fi
exec "$program" "$@"
