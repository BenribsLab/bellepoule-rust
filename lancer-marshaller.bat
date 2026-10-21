@echo off
rem Lance le Marshaller (gestion des pistes et des arbitres), compile si besoin.
rem BellePoule et le Marshaller se trouvent tout seuls sur le reseau local
rem (ou sur le meme ordinateur).
set "PATH=C:\Users\ben\gtk4\bin;%PATH%"
cd /d "%~dp0"
cargo run -p marshaller
