# BellePoule Rust — architecture du portage

Portage complet de **BellePoule 5.7** (C++/GTK2, branche `5.0/master`) en Rust/GTK4.

- Sources C++ de référence : `C:\Users\ben\Documents\dev\Bellepoule Rust\reference\v5.7\` (hors git, lecture seule).
- Ressources d'exécution copiées : `resources/` (images, pays, données localisées, traductions `.po`, anciens `.glade`).
- Fichiers d'exemple : `tests-data/` (`.cotcot`, FFF, XML FIE, classements).

## Objectif

Un clone **fonctionnellement identique** à la v5.7 :
- mêmes fonctionnalités, même ergonomie générale, mêmes libellés (traduits via `bp_core::i18n::tr`) ;
- **compatibilité des fichiers** : un `.cotcot` lu puis réenregistré doit rester lisible par la v5.7 C++ (mêmes balises, mêmes attributs, même ordre) ;
- **compatibilité réseau** : mêmes messages (`bp_net::Message`, format GKeyFile), mêmes ports, même chiffrement, pour dialoguer avec les applis smartphone et un Marshaller C++.
- Facebook/Twitter (OAuth) **ne sont pas portés**.

## Crates

| Crate | Rôle | Sources C++ |
|---|---|---|
| `bp-core` | Logique métier pure, sans GTK | `common/util` (hors widgets), `BellePoule/actors` (logique), `BellePoule/supervisor` (logique), `BellePoule/application/weapon` |
| `bp-net` | Réseau | `common/network` (hors facebook/twitter/oauth) |
| `bp-gtk` | Briques GTK4 communes : chargement `.ui`, `Module`, canevas cairo (remplace GooCanvas), impression, listes de joueurs, glisser-déposer | `common/util/{module,canvas,canvas_module,drop_zone,dnd_config,scroller,glade,...}` |
| `bellepoule` | Application principale (superviseur) | `BellePoule/supervisor`, `BellePoule/application` |
| `marshaller` | Gestion des pistes/arbitres/planning | `BellePoule/marshaller` |
| `livepoule` | Boîtier de score (Raspberry Pi) | `LivePoule` |
| `bp-usb-backend` | Détecteur de clés USB (processus séparé, JSON sur stderr) | `BellePoule/backend` |

## Fondation (déjà écrite, partagée)

- `bp_core::attribute` — descripteurs d'attributs (registre global fidèle à `Application::Prepare`), `Value`, comparaisons.
- `bp_core::player` — `Player`, `PlayerStore` (arène indexée par `PlayerRef = u32`), attributs locaux via `Owner::Stage(id)`.
- `bp_core::xml` — `XmlWriter` (sortie identique à libxml2) + aides de lecture `roxmltree`.
- `bp_core::stage` — trait `Stage`, `StageBase`, `StageContext`, `RawStage` (placeholder qui préserve le XML), `save_attendees`/`load_attendee`.
- `bp_core::bout` — `Score`, `Match`, `StartTime` (partagés poules/tableaux).
- `bp_core::i18n` — `tr()` et chargement des `.po`.
- `bp_net::message` — `Message` compatible GKeyFile.

### Règles sur la fondation
- Modifications **additives et rétrocompatibles uniquement** (nouvelles méthodes, méthodes par défaut dans les traits). Ne pas renommer ni changer une signature existante.
- Les dépendances se déclarent dans `[workspace.dependencies]` du `Cargo.toml` racine, puis `dep = { workspace = true }`. Ajouter une dépendance au workspace est permis.

## Conception

- Le C++ mélange logique et widgets dans chaque phase. Ici : **logique dans `bp-core`** (testable), **affichage dans `bellepoule`**, qui retrouve le type concret avec `stage.downcast_ref::<T>()`.
- Pas de callbacks d'observation dans le cœur : l'interface relit le modèle après chaque action.
- Les tirages aléatoires sont reproductibles (graine `rand_seed` de la phase, `player::random_key`).
- Dessin : GooCanvas est remplacé par une scène dessinée avec cairo (`bp-gtk::canvas`), réutilisée pour l'écran **et** l'impression/PDF.
- Interfaces : les `.glade` GTK2 sont convertis à la main en `.ui` GTK4 dans `resources/ui/` (même identifiants d'objets quand c'est possible, pour garder le code proche du C++).

## Propriétaires (vague 1)

Chaque agent ne modifie **que** ses fichiers (plus des ajouts au `Cargo.toml` racine).

| Agent | Fichiers |
|---|---|
| core-contest | `bp-core/src/{contest,tournament,schedule,weapon,category,fie_time,filter}.rs`, `bp-core/src/rounds/{mod,checkin,classification}.rs`, nouveaux sous-modules de ceux-ci, `bp-core/tests/contest_*.rs` |
| core-pool | `bp-core/src/rounds/pool/**`, `bp-core/tests/pool_*.rs` |
| core-table | `bp-core/src/rounds/{table/**,barrage.rs,splitting.rs}`, `bp-core/tests/table_*.rs` |
| core-formats | `bp-core/src/formats/**`, `bp-core/tests/formats_*.rs` |
| net | `crates/bp-net/**` |
| gui-foundation | `crates/bp-gtk/**`, `crates/bellepoule/**`, `resources/ui/` (hors fichiers marshaller/livepoule) |
| marshaller | `crates/marshaller/**`, `resources/ui/marshaller/` |
| livepoule | `crates/livepoule/**`, `crates/bp-usb-backend/**`, `resources/ui/livepoule/` |

## Compiler / lancer

GTK4 (build MSVC gvsbuild) est installé dans `C:\Users\ben\gtk4` ; `.cargo/config.toml` règle `PKG_CONFIG*`.
Pour **exécuter** un binaire GTK, ajouter `C:\Users\ben\gtk4\bin` au `PATH` :

```sh
PATH="/c/Users/ben/gtk4/bin:$PATH" cargo run -p bellepoule
```

Avant de rendre son travail : `cargo build --workspace`, `cargo test --workspace` et `cargo clippy` sans erreur, puis un commit git.
