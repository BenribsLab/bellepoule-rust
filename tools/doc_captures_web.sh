#!/usr/bin/env bash
# Captures web de la documentation : ManageComp Arbitre (téléphone) et
# ManageComp Live (écran de piste), avec le serveur local (docker compose).
#
# Crée un club de démonstration, y importe les compétitions d'exemple
# (tools/doc_captures.sh les fabrique), capture avec Edge sans fenêtre, puis
# retire le club. Utilisation (Git Bash) :  tools/doc_captures_web.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WEB="$ROOT/../Bellepoule Web"
OUT="$WEB/site/static/docs"
SAMPLES="$ROOT/docs-captures/samples"
SERVER="http://127.0.0.1:${BP_PORT:-8090}"
EDGE="/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
ADMIN="$(grep '^BP_ADMIN_KEY=' "$WEB/.env" | cut -d= -f2)"
PROFILE="$(cygpath -w "$ROOT/docs-captures/edge")"
mkdir -p "$OUT"

# (sys.stdout.write : pas de fin de ligne Windows dans les variables)
json() { python -c "import sys,json; d=json.load(sys.stdin); sys.stdout.write(str($1))"; }

# Corps JSON écrit en UTF-8 par Python (la ligne de commande Windows abîme les accents).
body() { python -c "import sys,json,tempfile; f=tempfile.NamedTemporaryFile('w',encoding='utf-8',suffix='.json',delete=False); json.dump(json.loads(sys.argv[1]),f,ensure_ascii=False); sys.stdout.write(f.name)" "$1"; }

# Club de démonstration
CLIENT=$(curl -s -X POST "$SERVER/api/admin/clients" -H "x-api-key: $ADMIN" -H "content-type: application/json" --data-binary "@$(body '{"name": "Club de démonstration"}')")
KEY=$(echo "$CLIENT" | json 'd["key"]')
ID=$(echo "$CLIENT" | json 'd["id"]')
for f in jeunes equipes; do
    curl -s -o /dev/null -X POST "$SERVER/api/contests/import" -H "x-api-key: $KEY" --data-binary "@$SAMPLES/$f.cotcot"
done
ADMINCODE=$(curl -s "$SERVER/api/smartpoule/admin" -H "x-api-key: $KEY")
CLUB=$(echo "$ADMINCODE" | json 'd["club"]')
REFKEY=$(echo "$ADMINCODE" | json 'd["key"]')
ID_Q="c=$CLUB&r=0&k=$REFKEY"
JOBS=$(curl -s "$SERVER/api/smartpoule/phone/jobs?$ID_Q")
POOL=$(echo "$JOBS" | json '[j["token"] for j in d["jobs"] if j["kind"]=="pool"][0]')
TEAM=$(echo "$JOBS" | json '[j["token"] for j in d["jobs"] if j["kind"]=="match"][0]')
enc() { python -c "import sys,urllib.parse; sys.stdout.write(urllib.parse.quote(sys.argv[1], safe=''))" "$1"; }

shot() { # nom largeur hauteur adresse
    rm -f "$OUT/$1.png"
    "$EDGE" --headless=new --disable-gpu --hide-scrollbars --user-data-dir="$PROFILE" \
        --window-size="$2,$3" --virtual-time-budget=8000 --screenshot="$(cygpath -w "$OUT/$1.png")" "$4" >/dev/null 2>&1
    [ -s "$OUT/$1.png" ] && echo "ok  $1" || echo "ÉCHEC $1"
}

# Edge sans fenêtre impose une largeur minimale (492 px de page) : c'est
# celle d'un grand téléphone.
phone() { shot "$1" 492 1000 "$2"; }

APP="$SERVER/smartpoule/?$ID_Q"
phone arbitre-liste "$APP#/"
phone arbitre-feuille "$APP#/feuille/$(enc "$POOL")"
# Premier match de la poule pas encore joué
SHEET=$(curl -s "$SERVER/api/smartpoule/phone/sheet?$ID_Q&token=$(enc "$POOL")")
BOUT=$(echo "$SHEET" | json '[b["index"] for b in d["bouts"] if not b["over"]][0]')
phone arbitre-match "$APP#/match/$(enc "$POOL")/$BOUT"
phone arbitre-relais "$APP#/match/$(enc "$TEAM")/0"

# Écran de piste : un match en cours sur la piste 1
LIVE=$(body "{\"c\": \"$CLUB\", \"r\": 0, \"k\": \"$REFKEY\", \"piste\": 1, \"competition\": \"Entraînement des jeunes d'octobre\", \"color\": \"#3d6fd6\", \"bout\": \"Groupe 1 · Poule n° 01 · M4\", \"red\": {\"name\": \"MARTIN Lina\", \"score\": \"3\"}, \"green\": {\"name\": \"DUBOIS Gabriel\", \"score\": \"2\"}, \"remaining_ms\": 107000, \"running\": false, \"period\": 1, \"periods\": 1, \"over\": false}")
curl -s -o /dev/null -X POST "$SERVER/api/smartpoule/phone/live" -H "content-type: application/json" --data-binary "@$LIVE"
shot live 1280 720 "$SERVER/live/?c=$CLUB&p=1"

# Le club de démonstration est retiré (ses compétitions restent sur le disque du conteneur).
curl -s -o /dev/null -X DELETE "$SERVER/api/admin/clients/$ID" -H "x-api-key: $ADMIN"
