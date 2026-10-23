@echo off
rem Lance l'ecran de piste ManageComp Live (compile si besoin).
rem Premiere fois : lancer-livepoule.bat --live "http://serveur/live/?c=CLUB" --piste 1
rem (adresse dans ManageComp > Organisation > Arbitres sur telephone...) ; ensuite les reglages sont memorises.
rem Touches + et - du pave numerique : changer de piste. Echap : quitter.
set "PATH=C:\Users\ben\gtk4\bin;%PATH%"
cd /d "%~dp0"
cargo run -p livepoule -- %*
