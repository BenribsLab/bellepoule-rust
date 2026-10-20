@echo off
rem Lance BellePoule (compile si besoin). Fonctionne dans cmd et PowerShell.
rem Utilisation : lancer.bat [fichier.cotcot]
set "PATH=C:\Users\ben\gtk4\bin;%PATH%"
cd /d "%~dp0"
cargo run -p bellepoule -- %*
