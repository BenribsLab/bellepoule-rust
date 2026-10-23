#!/usr/bin/env bash
# Captures d'écran de la documentation du site (managecomp.org).
#
# 1. compétitions d'exemple (noms fictifs) ;
# 2. une capture par vue, avec le mode --doc-capture du logiciel ;
# 3. images déposées dans ../Bellepoule Web/site/static/docs/.
#
# Utilisation (Git Bash, depuis la racine du dépôt) :  tools/doc_captures.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/../Bellepoule Web/site/static/docs"
SAMPLES="$ROOT/docs-captures/samples"
EXE="$ROOT/target/agent/debug/managecomp.exe"
export PATH="/c/Users/ben/gtk4/bin:$PATH"
export CARGO_TARGET_DIR="$ROOT/target/agent"

# Profil de réglages isolé : ni compétitions récentes ni arbitres de
# l'utilisateur dans les images.
PROFILE="$ROOT/docs-captures/profile"
rm -rf "$PROFILE"
mkdir -p "$OUT" "$SAMPLES" "$PROFILE/roaming" "$PROFILE/xdg" "$PROFILE/organisation"
export APPDATA="$(cygpath -w "$PROFILE/roaming")"
export XDG_CONFIG_HOME="$(cygpath -w "$PROFILE/xdg")"
export BELLEPOULE2D_CONFIG_DIR="$(cygpath -w "$PROFILE/organisation")"
# Organisation : arbitres et pistes gérés.
mkdir -p "$PROFILE/roaming/BellePoule"
printf '[Organisation]
referees=1
pistes=1
' > "$PROFILE/roaming/BellePoule/config.ini"
cargo build -q -p bellepoule || exit 1
cargo run -q -p bp-core --example doc_samples -- "$SAMPLES" >/dev/null || exit 1

shot() { # nom vue phase fichier [taille]
    local name="$1" view="$2" stage="$3" file="$4" size="${5:-1400x900}"
    local args=(--doc-capture "$OUT/$name.png" --view "$view" --size "$size")
    [ -n "$stage" ] && args+=(--stage "$stage")
    [ -n "$file" ] && args+=("$SAMPLES/$file")
    rm -f "$OUT/$name.png"
    timeout 30 "$EXE" "${args[@]}" </dev/null >/dev/null 2>&1
    [ -s "$OUT/$name.png" ] && echo "ok  $name" || echo "ÉCHEC $name"
}

shot proprietes         properties   ""          individuel.cotcot
shot formule            properties   poules      individuel.cotcot
shot appel              stage        appel       individuel.cotcot
shot poules-composition stage        composition individuel.cotcot
shot poules-saisie      stage        poules      individuel.cotcot
shot tableau            stage        tableau     individuel.cotcot
shot organisation       organisation ""          individuel.cotcot
shot jeunes-groupes     stage        jeunes      jeunes.cotcot
shot jeunes-poules      stage        poules      jeunes.cotcot
shot jeunes-impression  print-groups ""          jeunes.cotcot
shot equipes-appel      stage        appel       equipes.cotcot
shot equipes-preferences properties  tableau     equipes.cotcot
shot equipes-tableau    stage        tableau     equipes.cotcot
shot equipes-feuille    print-relay  ""          equipes.cotcot
shot accueil            home         ""          ""
