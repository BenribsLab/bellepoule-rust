# Visuels de l'interface

Créés le 21 octobre 2026 avec l'outil intégré **imagegen**, puis copiés dans le projet et utilisés
par `visuals.rs` et `shell.rs`. La maquette fournie par l'utilisateur guide la structure de l'application.
Les PNG sont des éléments décoratifs ; toutes les commandes et données restent des widgets GTK.

| Fichier | Utilisation |
|---|---|
| `fencing-header.png` | Bandeau de compétition et accueil ; photographie de matériel d'escrime |
| `fencers-sidebar.png` | Illustration de deux tireurs, avec transparence, en bas de la navigation |
| `icons/*.svg` | Icônes de navigation dessinées en code, traits cohérents, sans génération raster |

## Prompts utilisés

### Bandeau

```text
Use case: product-mockup. Asset type: premium fencing tournament desktop application header background, wide landscape banner 3:1 composition. Create an elegant photorealistic still life of real sport fencing equipment: a black mesh fencing mask, a white fencing glove, a foil with circular silver bell guard and slim blade, resting on a pale blue-gray surface. Equipment is grouped in the RIGHTMOST third of the image, softly lit studio product photography, realistic fine steel and woven cloth details. The LEFT TWO THIRDS are clean very pale cool white negative space, gradually blending into the blue-gray background on the right, suitable for dark blue application headings placed separately in code. Palette: white, mist blue, navy, silver. Refined contemporary European sports management app visual. No text, no letters, no logo, no watermark, no interface elements. Landscape image.
```

### Illustration latérale

```text
Use case: stylized-concept. Asset type: small decorative sidebar illustration for a sophisticated fencing tournament desktop app. Two sport fencers facing each other in a fencing bout, wearing masks and protective uniforms, one in a graceful lunge and one parrying, full body, horizontal composition. Refined minimal editorial illustration with elegant flowing shapes, subtle fine linework, muted slate blue and pale blue, very low contrast, tasteful premium software illustration, clean readable silhouettes even at 160 pixels wide. Isolated on genuinely transparent background with no floor rectangle, no background scene. No text, no letters, no watermark, no logo.
```
