# Bildwürfel: Standardset und Grafikformat

Stand: 2026-10-10. Das SVG-Set ist als „Geschichten · Classic“ in der App implementiert; die folgenden Quellen dokumentieren seine Referenz und Grenzen.

## Empfehlung

Eigenes Set **„Geschichten · Classic“**: neun unterschiedliche D6 mit je sechs festen Motiven, orientiert am belegten alten Originalset. **SVG als Quelldateien**, Lucide-Linienstil plus eigene Ergänzungen. Einzelne Assets lokal, keine neue Laufzeitbibliothek. 2D zeigt SVG-Bilder direkt, 3D rasterisiert sie in den vorhandenen Canvas-Atlas. Feste helle Würfelkörper und dunkle Motive; kein Editor für eigene Farben, Texturen oder Bilder.

## 1. Belegte Würfelgruppen

Die [archivierte Sammlerliste von Onyx, Post #2 (2015)](https://web.archive.org/web/20150316212151/http://tarotforum.net/showthread.php?t=231047) enthält alle 9×6 Motive. Die Live-Seite antwortet mit HTTP 403. Würfel-/Seitennummern sind laut Autorin willkürlich; eine ältere Ursprungsliste wurde nicht wiedergefunden.

Ein unabhängiges [Foto aller Seiten des Originalsets, BGG 1300800 (2012)](https://boardgamegeek.com/image/1300800) zeigt zwei Dreieransichten je Würfel. Alle neun Gruppen stimmen mit der Liste überein. Gleiche Rasterpositionen links/rechts als gleicher Würfel ist eine durch den Abgleich gestützte Interpretation [INFERENCE], keine Herstellerbestätigung. Archivtext und Foto direkt geprüft; Originalgrafiken nicht als App-Assets übernommen.

| Würfel | Sechs Motive, Sammler-Reihenfolge |
|---|---|
| W1 | Haus · Glühbirne · Schlafendes Gesicht · Sprechblase · Uhr · Gefiederter Pfeil |
| W2 | Vorhängeschloss · Fußabdruck · Flamme · Schaf · Magnet · L-Fahrschülerschild |
| W3 | Nachricht/Karte · Pyramide · Turm · Regenbogen · Baum · Auge |
| W4 | Globus · Flugzeug · Lächelndes Gesicht · Hochhaus · Taschenlampe · Apfel |
| W5 | Schlüssel · Sternschnuppe · Fragezeichen · Zelt · Drehendes Objekt („Fountain“, unklar) · Trauriges Gesicht |
| W6 | Skarabäus/Käfer · Spielwürfel · Lupe · Schattengestalt · Hand · Schildkröte |
| W7 | Theatermasken · Fisch · Fallschirm · Schlüsselloch · Sonnenblume · Pfeilstern |
| W8 | Buch · Brücke · Abakus · Mondsichel · Zauberstab · Biene |
| W9 | Alien · Blitz · Handy · Waage · Richtungspfeil · Gehstock |

**W1–W9 sind Referenz-IDs, keine offiziellen Würfelnummern.** Reihenfolge beschreibt keine räumliche Würfelabwicklung. Exakte Gegenüber-Paare nicht ermittelt; für unser eigenes Set können wir die Seitenpositionen selbst definieren.

Vollständiger Abgleich gilt für das **alte Original** (Beilage im Foto ©2007 The Creativity Hub). [Asmodee Classic](https://www.asmodee.de/produkte/rorys-story-cubes-classic) bestätigt neun Würfel; die [Regel-PDF](https://asmodee-resources.azureedge.net/media/germanyprod/Regeln/rorys-story-cubes-classic-3558380127130-regel.pdf) enthält keine vollständige Motivzuordnung. Neuere Fotos sind konsistent: [deutsche Classic-Ausgabe](https://boardgamegeek.com/image/6739927), [französische Zygomatic-Ausgabe](https://boardgamegeek.com/image/5161880), [englische Classic-Box](https://boardgamegeek.com/image/7714167). Stichproben beweisen nicht, dass heute alle 54 Motive/Gruppen unverändert sind.

W2 „L“ ist laut Forum Post #9 ein Fahrschülerschild, kein Briefkasten. W6 „Spyglass“ zeigt eine Lupe. W7 „Outer“ ist ein achtarmiger Pfeilstern, keine Schneeflocke. W3 „Message“ ist eine Karte mit Textzeilen; Brief/Ticket/Ausweis sind Interpretationen. W5 „Fountain“ wirkt wie ein rotierendes rundes Objekt; Brunnen/Münze/Kreisel bleiben unentschieden. Dafür kein Brunnen-Icon als exakte Übernahme festschreiben.

## 2. Motiv → Ersatzgrafik

✅ motivisch brauchbarer Kandidat, keine Originalkopie; ≈ Näherung; ⊕ Komposit; ✗ kein geeigneter Einzelkandidat. Die Tabelle dokumentiert die Recherchekandidaten. Die tatsächlich verwendeten 38 Lucide-Motive und 16 eigenen SVGs sind in `src/model.rs` vermerkt. Die Reihenfolge dieser Tabelle folgt den Fotoansichten, nicht den Sammler-Seitennummern.

Kandidaten anhand der Primär-Repositories geprüft und als Kontaktbogen gesichtet. Gepinnte Quellen: [Lucide 70562c1e](https://github.com/lucide-icons/lucide/tree/70562c1ee1c4fdcf736fe97bc893fb8511927934/icons), [Game-icons 82d94881](https://github.com/game-icons/icons/tree/82d948812bfe3f269ef8f731dcdb07b08160edc4). Motivquelle bleibt der Foto-/Forumabgleich.

| W | Motiv | Lucide | Einschränkung | Game-icons | Einschränkung |
|---|---|---|---|---|---|
| W1 | Glühbirne | [`lightbulb`](https://lucide.dev/icons/lightbulb) ✅ |  | [`lorc/light-bulb`](https://game-icons.net/1x1/lorc/light-bulb.html) ✅ |  |
| W1 | Schlaf-Gesicht „Zz“ | — ⊕ | Kein Schlaf-Gesicht; Komposit aus Gesichtskreis mit geschlossenen Augen + „z z“, selbst zeichnen. `bed` wäre nur eine Ersatzidee | [`lorc/sleepy`](https://game-icons.net/1x1/lorc/sleepy.html) ≈ | Gesicht mit geschlossenen Augen, ohne Zz; Alternative `delapouite/night-sleep` (Mond + Zzz) |
| W1 | Pfeil, gefiedert, senkrecht | [`arrow-down`](https://lucide.dev/icons/arrow-down) ≈ | ohne Befiederung; muss sich von W9 unterscheiden → besser eigener gefiederter Pfeil oder `bow-arrow` (mit Bogen) | [`lorc/broadhead-arrow`](https://game-icons.net/1x1/lorc/broadhead-arrow.html) ✅ | gefiederter Pfeil, senkrecht drehen |
| W1 | Haus | [`house`](https://lucide.dev/icons/house) ✅ |  | [`delapouite/house`](https://game-icons.net/1x1/delapouite/house.html) ✅ |  |
| W1 | Sprechblase | [`message-circle`](https://lucide.dev/icons/message-circle) ✅ |  | [`delapouite/chat-bubble`](https://game-icons.net/1x1/delapouite/chat-bubble.html) ✅ |  |
| W1 | Uhr | [`clock`](https://lucide.dev/icons/clock) ✅ | Feather/MIT | [`delapouite/alarm-clock`](https://game-icons.net/1x1/delapouite/alarm-clock.html) ≈ | Wecker statt Uhr; Alternative `skoll/pocket-watch` |
| W2 | Schaf | — ✗ | Custom | [`delapouite/sheep`](https://game-icons.net/1x1/delapouite/sheep.html) ✅ |  |
| W2 | Hufeisenmagnet | [`magnet`](https://lucide.dev/icons/magnet) ✅ |  | [`lorc/magnet`](https://game-icons.net/1x1/lorc/magnet.html) ✅ |  |
| W2 | Fußabdruck (barfuß) | [`footprints`](https://lucide.dev/icons/footprints) ≈ | zwei Schuhsohlen, kein nackter Fuß | [`lorc/footprint`](https://game-icons.net/1x1/lorc/footprint.html) ✅ | nackter Fuß |
| W2 | Vorhängeschloss | [`lock`](https://lucide.dev/icons/lock) ✅ | Feather/MIT | [`lorc/padlock`](https://game-icons.net/1x1/lorc/padlock.html) ✅ |  |
| W2 | L-Schild (Fahrschüler) | — ✗ | Custom: Quadrat + „L“ (trivial) | — ✗ | Custom |
| W2 | Flamme | [`flame`](https://lucide.dev/icons/flame) ✅ |  | [`carl-olsen/flame`](https://game-icons.net/1x1/carl-olsen/flame.html) ✅ | Alternative `sbed/fire` |
| W3 | Baum | [`tree-deciduous`](https://lucide.dev/icons/tree-deciduous) ✅ |  | [`lorc/oak`](https://game-icons.net/1x1/lorc/oak.html) ✅ |  |
| W3 | Pyramide | [`pyramid`](https://lucide.dev/icons/pyramid) ≈ | geometrischer Körper, keine ägyptische Pyramide | [`delapouite/great-pyramid`](https://game-icons.net/1x1/delapouite/great-pyramid.html) ✅ |  |
| W3 | Karte mit Textzeilen + 2 Kästchen (Deutung offen) | [`id-card`](https://lucide.dev/icons/id-card) ≈ | passt zum Bild (Karte, Zeilen, Personenkästchen), falls Ausweis gemeint ist; bei Brief-Deutung `mail` | [`delapouite/id-card`](https://game-icons.net/1x1/delapouite/id-card.html) ≈ | Ausweis; Alternative `lorc/envelope` |
| W3 | Regenbogen | [`rainbow`](https://lucide.dev/icons/rainbow) ✅ |  | — ✗ | `lorc/rainbow-star` ist ein anderes Motiv |
| W3 | Auge | [`eye`](https://lucide.dev/icons/eye) ✅ |  | [`lorc/eyeball`](https://game-icons.net/1x1/lorc/eyeball.html) ≈ | detailliert/gruselig; `badges/eye` meiden |
| W3 | Turm (mit Zinnen) | [`chess-rook`](https://lucide.dev/icons/chess-rook) ✅ | Alternative `castle` | [`lorc/stone-tower`](https://game-icons.net/1x1/lorc/stone-tower.html) ✅ |  |
| W4 | Lächelndes Gesicht | [`face-slightly-smiling`](https://lucide.dev/icons/face-slightly-smiling) ✅ | früher `smile`, Feather-Liste | — ✗ | Custom |
| W4 | Hochhaus | [`building`](https://lucide.dev/icons/building) ✅ | hohes Rechteck mit Fenstern | [`delapouite/modern-city`](https://game-icons.net/1x1/delapouite/modern-city.html) ≈ | mehrere Gebäude |
| W4 | Globus | [`earth`](https://lucide.dev/icons/earth) ✅ |  | [`delapouite/earth-africa-europe`](https://game-icons.net/1x1/delapouite/earth-africa-europe.html) ✅ |  |
| W4 | Flugzeug | [`plane`](https://lucide.dev/icons/plane) ✅ |  | [`skoll/airplane`](https://game-icons.net/1x1/skoll/airplane.html) ✅ | Propellerflugzeug |
| W4 | Taschenlampe | [`flashlight`](https://lucide.dev/icons/flashlight) ✅ |  | [`delapouite/flashlight`](https://game-icons.net/1x1/delapouite/flashlight.html) ✅ |  |
| W4 | Apfel | [`apple`](https://lucide.dev/icons/apple) ✅ |  | [`lorc/shiny-apple`](https://game-icons.net/1x1/lorc/shiny-apple.html) ✅ |  |
| W5 | Sternschnuppe | [`sparkles`](https://lucide.dev/icons/sparkles) ≈ | Alternative `star`; ohne Schweif | [`delapouite/falling-star`](https://game-icons.net/1x1/delapouite/falling-star.html) ✅ |  |
| W5 | Drehendes Objekt mit gestrichelten Bögen („Fountain“, Deutung unsicher) | — ✗ | je nach Deutung `coins` (Münze) ≈; selbst zeichnen nach eigener Deutung empfohlen | — ✗ | je nach Deutung `caro-asercion/coinflip`, `skoll/spinning-top` oder `delapouite/water-fountain` ≈; keine Deutung behaupten |
| W5 | Fragezeichen | [`circle-question-mark`](https://lucide.dev/icons/circle-question-mark) ✅ |  | [`lorc/uncertainty`](https://game-icons.net/1x1/lorc/uncertainty.html) ≈ | ? mit Kreispfeilen |
| W5 | Trauriges Gesicht | [`face-slightly-frowning`](https://lucide.dev/icons/face-slightly-frowning) ✅ | früher `frown`, Feather-Liste | — ✗ | Custom |
| W5 | Zelt (Reißverschluss-Tür) | [`tent`](https://lucide.dev/icons/tent) ✅ | Dreieckszelt mit Tür | [`delapouite/camping-tent`](https://game-icons.net/1x1/delapouite/camping-tent.html) ✅ |  |
| W5 | Schlüssel | [`key`](https://lucide.dev/icons/key) ✅ | Feather/MIT; muss sich von W2-Schloss und W7-Schlüsselloch unterscheiden | [`sbed/key`](https://game-icons.net/1x1/sbed/key.html) ✅ | Alternative `lorc/skeleton-key` |
| W6 | Hand | [`hand`](https://lucide.dev/icons/hand) ✅ |  | [`lorc/hand`](https://game-icons.net/1x1/lorc/hand.html) ✅ |  |
| W6 | Schildkröte | [`turtle`](https://lucide.dev/icons/turtle) ✅ |  | [`lorc/turtle`](https://game-icons.net/1x1/lorc/turtle.html) ✅ |  |
| W6 | Skarabäus/Käfer | [`bug`](https://lucide.dev/icons/bug) ≈ | Käfer, Skarabäus nicht eindeutig | [`lorc/scarab-beetle`](https://game-icons.net/1x1/lorc/scarab-beetle.html) ✅ |  |
| W6 | Schattengestalt (Stalking/Monster) | [`ghost`](https://lucide.dev/icons/ghost) ≈ | Gespenst, keine Schattengestalt; Custom empfohlen | [`lorc/shadow-grasp`](https://game-icons.net/1x1/lorc/shadow-grasp.html) ≈ | Alternative `lorc/spectre`; Custom empfohlen |
| W6 | Lupe | [`search`](https://lucide.dev/icons/search) ✅ | Feather/MIT | [`lorc/magnifying-glass`](https://game-icons.net/1x1/lorc/magnifying-glass.html) ✅ |  |
| W6 | Spielwürfel | [`dices`](https://lucide.dev/icons/dices) ✅ | Alternative `dice-5` | [`delapouite/perspective-dice-six-faces-random`](https://game-icons.net/1x1/delapouite/perspective-dice-six-faces-random.html) ✅ | Würfel mit ?-Flächen |
| W7 | Fisch | [`fish`](https://lucide.dev/icons/fish) ✅ |  | [`lorc/fishbone`](https://game-icons.net/1x1/lorc/fishbone.html) ≈ | Gräte; weitere GI-Fische nicht visuell geprüft |
| W7 | Pfeilstern (8 Pfeile nach außen) | [`move`](https://lucide.dev/icons/move) + [`expand`](https://lucide.dev/icons/expand) ⊕ | Komposit: `move` (4 Achsenpfeile, Feather/MIT) + `expand` (4 Diagonalpfeile) übereinander ergibt 8 Pfeile nach außen; Ergebnis prüfen | [`lorc/divergence`](https://game-icons.net/1x1/lorc/divergence.html) ≈ | nur 3 Pfeile nach oben; Alternative `delapouite/expand` (4 Diagonalpfeile); Custom |
| W7 | Schlüsselloch (schwarz) | [`lock-keyhole`](https://lucide.dev/icons/lock-keyhole) ≈ | Schloss mit Punkt; Custom (Kreis + Trapez) trivial | — ✗ | Custom |
| W7 | Sonnenblume | [`flower-2`](https://lucide.dev/icons/flower-2) ✅ | Blume mit Stiel und Blättern; `flower` = Blüte allein | [`delapouite/sunflower`](https://game-icons.net/1x1/delapouite/sunflower.html) ✅ |  |
| W7 | Theatermasken | [`drama`](https://lucide.dev/icons/drama) ✅ | fein bei kleiner Größe | [`lorc/drama-masks`](https://game-icons.net/1x1/lorc/drama-masks.html) ✅ | fein bei kleiner Größe |
| W7 | Fallschirm | — ✗ | Custom | [`lorc/parachute`](https://game-icons.net/1x1/lorc/parachute.html) ✅ |  |
| W8 | Zauberstab mit Sternen | [`wand-sparkles`](https://lucide.dev/icons/wand-sparkles) ✅ |  | [`lorc/fairy-wand`](https://game-icons.net/1x1/lorc/fairy-wand.html) ✅ |  |
| W8 | Biene | — ✗ | `bug` ist Käfer → Custom | [`lorc/bee`](https://game-icons.net/1x1/lorc/bee.html) ✅ |  |
| W8 | Brücke (Bogen-/Hängebrücke) | [`bridge`](https://lucide.dev/icons/bridge) ✅ | Hängebrücke | [`delapouite/stone-bridge`](https://game-icons.net/1x1/delapouite/stone-bridge.html) ≈ | Bogenbrücke |
| W8 | Abakus | — ✗ | `calculator` nur schwach → Custom | [`delapouite/abacus`](https://game-icons.net/1x1/delapouite/abacus.html) ✅ |  |
| W8 | Mondsichel | [`moon`](https://lucide.dev/icons/moon) ✅ | Feather/MIT | [`lorc/moon`](https://game-icons.net/1x1/lorc/moon.html) ✅ |  |
| W8 | Offenes Buch | [`book-open`](https://lucide.dev/icons/book-open) ✅ |  | [`lorc/open-book`](https://game-icons.net/1x1/lorc/open-book.html) ✅ |  |
| W9 | Handy (Tastenhandy) | [`smartphone`](https://lucide.dev/icons/smartphone) ≈ | Smartphone ohne Tasten; Feather/MIT | [`delapouite/smartphone`](https://game-icons.net/1x1/delapouite/smartphone.html) ≈ | Smartphone; Custom-Tastenhandy möglich |
| W9 | Richtungspfeil diagonal | [`arrow-down-right`](https://lucide.dev/icons/arrow-down-right) ✅ | Feather/MIT; muss sich klar von W1 unterscheiden | [`delapouite/plain-arrow`](https://game-icons.net/1x1/delapouite/plain-arrow.html) ≈ | massiver Blockpfeil, diagonal drehen |
| W9 | Waage | [`scale`](https://lucide.dev/icons/scale) ✅ |  | [`lorc/scales`](https://game-icons.net/1x1/lorc/scales.html) ✅ | Balkenwaage |
| W9 | Blitz | [`zap`](https://lucide.dev/icons/zap) ✅ |  | [`lorc/focused-lightning`](https://game-icons.net/1x1/lorc/focused-lightning.html) ✅ |  |
| W9 | Alien-Gesicht | — ✗ | `ghost` falsch → Custom | [`lorc/alien-stare`](https://game-icons.net/1x1/lorc/alien-stare.html) ✅ | ovaler Kopf, große Augen |
| W9 | Gehstock | — ✗ | Custom (J-Form) | — ✗ | `lorc/wizard-staff` falsch → Custom |

### Stil und Lizenzen

**Lucide plus eigene SVGs im selben Stil.** Ergänzungen insbesondere für Schaf, Biene, Abakus, Alien, Fallschirm, L-Schild, Gehstock, Schlafgesicht, isoliertes Schlüsselloch und Pfeilstern. W5 erst nach bewusster Motiventscheidung; optional gefiederter Pfeil, Sternschnuppe, Tastenhandy und Schattengestalt statt Näherungen. Keine vermeintlich exakte Abdeckungsquote: Bedeutungsnähe bleibt eine Produktentscheidung.

- [Lucide-Lizenz](https://github.com/lucide-icons/lucide/blob/70562c1ee1c4fdcf736fe97bc893fb8511927934/LICENSE): ISC; Feather-abgeleitete Icons zusätzlich MIT. Vollständige Lizenzdatei unverändert mit den SVGs ausliefern; keine zusätzliche Pflicht für sichtbare Autorenliste.
- [Game-icons](https://game-icons.net/about.html): CC BY 3.0; konkrete Autoren, Quelle/Lizenz nennen, Bearbeitungen kennzeichnen ([Bedingungen](https://creativecommons.org/licenses/by/3.0/legalcode)). Hier Lorc, Delapouite, Skoll, Carl Olsen und sbed; zusätzliche Kandidaten von Caro Asercion ebenfalls entsprechend nennen, falls übernommen. Viele SVGs enthalten schwarzen Hintergrund/weißes Motiv: für transparente schwarze Symbole lokal anpassen. Silhouetten nicht ungeprüft mit Lucide-Outline mischen.
- Bei externen SVG-Bildern explizite Strich-/Füllfarben: `currentColor` erbt nicht die CSS-Farbe des umgebenden HTML-Elements. Keine Fonts, Filter, Skripte oder externen Ressourcen. [SVG als Bild](https://developer.mozilla.org/en-US/docs/Web/SVG/Guides/SVG_as_an_image).
- Ersatzgrafiken geben keine Lizenz an Originalillustrationen, Fotos oder Marke „Rory’s Story Cubes“. Eigener Name/eigene Motive vermeiden deren direkte Übernahme, garantieren aber keine rechtliche Freigabe der gesamten Nachbildung.

## 3. Performance: SVG vs. Raster und Pixel-Art

`src/gpu.rs` baut einen Canvas-Atlas, cached decodierte Bilder und GPU-Texturen in begrenztem LRU und lädt Texturen samt Mipmaps. [Canvas drawImage](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/drawImage) zeichnet SVG-Bildquellen; wgpu überträgt den Canvas an WebGPU oder WebGL2.

| Aspekt | SVG | PNG / WebP |
|---|---|---|
| Download/Offline-Größe | Einfache Linienmotiven oft klein | Motiv, Auflösung und Encoder entscheidend; komplexe Bilder ggf. kleiner |
| Erstes Anzeigen | XML/Pfade lesen und rasterisieren | Rasterbild decodieren; bei komplexen SVGs ggf. günstiger |
| 2D-Skalierung | Ein Asset für unterschiedliche Größen/Displays | Ausreichende Pixelauflösung nötig, sonst unscharf |
| 3D nach Atlas-Aufbau | Rastertextur, keine SVG-Pfade pro Frame | Gleiche Rastertextur bei gleicher Atlasgröße/GPU-Format |
| GPU-Speicher | Atlasauflösung/-format entscheidend | Dateikompression bleibt beim RGBA-Canvas-Upload nicht als GPU-Kompression erhalten |

**Cache und Atlas-Auflösung sind wichtiger als die Dateiendung.** D6-Atlas aktuell 2048×256 Pixel: RGBA8 rechnerisch 2 MiB Basisdaten, mit Mipmaps etwa 2,67 MiB. Neun getrennte Story-Würfelatlanten: etwa **24 MiB GPU-Texturdaten**. LRU erlaubt 16 Atlanten, bei dieser Größe etwa 42,7 MiB. Decodierte Bilder, Canvas-Zwischenspeicher, Treiber-Overhead und weitere Ressourcen nicht enthalten. Berechnung, keine Speichermessung; für SVG und PNG gleich.

`src/physics.rs` randomisiert vor jedem Wurf die geometrische Seitenzuordnung. Cache-Key enthält die Permutation. Story-Würfel behalten ihre sechs Motive **je individuellem Würfel**, keine Auswahl aus einem gemeinsamen 54er-Pool. Nicht für vermeintlich festen Cache die Fairness-Randomisierung abschalten. Würfel-ID zusätzlich in Cache-Key, sonst würden alle neun D6 dieselben Motive erhalten.

Erste Implementierung: Bilder pro URL einmal decodieren, alle sechs vor fertigem Atlasaufbau bereitstellen, vorhandenen Atlas-LRU nutzen. Keine neuen Bildobjekte/Uploads in normalen Animationsframes. Neue Permutation/Darstellung oder Context-Restore können neuen Atlas erfordern. Unveränderlicher Atlas mit separater UV-Permutation ist eine größere Rendereränderung, nur bei nachgewiesenem Engpass.

### Kleiner gemessener Formatvergleich

Chrome 155 unter macOS: einfaches Lucide-Sonnenmotiv, SVG explizit 256×256/schwarze Striche; daraus PNG 256×256 im Browser erzeugt. Dasselbe Motiv 54-mal in neun 2048×256-Atlanten platziert. Upload über echte `DiceRenderer.upload()`-Methode. **Keine Messung von 54 unterschiedlichen Motiven oder implementiertem Story-Set.**

| Messung | SVG | PNG |
|---|---:|---:|
| Dateigröße, ohne Übertragungskompression | 482 B | 9.113 B |
| Median Bild-Laden inkl. Decode aus lokalen Blobs (21 Durchläufe) | 0,4 ms | 1,4 ms |
| Median Zeichen-Aufrufe für neun Atlanten (25 nach Warmup) | 0,1 ms | unter Timerauflösung (0,0 ms) |
| Median neun Uploads inkl. Mipmap-Aufrufen/GPU-finish | 0,1 ms | 0,1 ms |

Kurze Draw-/Uploadzeiten liegen an Timerauflösung: **keine belastbare FPS-Rangfolge**; Canvas-Arbeit kann verzögert ausgeführt werden. Keine Netzwerk-/gzip-/WebP-Messung, PNG nicht extern optimiert. Sichtprüfung bei 48/96/192 CSS-Pixeln: beide gut erkennbar, skaliert nicht pixelidentisch. WebGL-Fehlercode nach Vergleich: 0.

Der kleine Formatvergleich entstand vor der Implementierung. Die Verifikation des fertigen Sets steht unten. **Nicht geprüft:** reale Android-/iPhone-Geräte, Safari, Mobile-GPU-Speicher oder Framerate auf Zielhandys.

### Pixel-Art ist eine Stilentscheidung

PNG/WebP kann glatte Illustrationen oder Pixel-Art enthalten. Pixel-Art braucht bewusst gezeichnete Motive und kontrollierte Skalierung. 2D: `image-rendering: pixelated`; 3D: aktuelle lineare Filter/Mipmaps glätten. Harte Pixeloptik braucht zusätzlich passenden Umgang mit Canvas-Smoothing und Texturfilterung, bei schrägen Flächen mit Aliasing-Risiko. Kein automatischer Performancegewinn.

**Entscheidung:** einfache SVGs für beide Ansichten. Vorgerenderte PNG-/verlustfreie WebP-Assets erst erwägen, wenn das fertige Set auf Zielhandys einen messbaren Decode-/Rasterisierungsengpass zeigt.

## 4. Implementierung und Verifikation

- `src/model.rs`: neun feste Gruppen `classic-1`–`classic-9` in Sammler-Reihenfolge, 54 lokale SVG-Pfade mit Namen für alle acht App-Sprachen. 38 Lucide-Motive, 16 eigenständig gezeichnete Ergänzungen; vollständige Lucide-/Feather-Lizenz im Assetordner.
- W5 zeigt bewusst ein neutrales drehendes Objekt, ohne eine unsichere Brunnen-/Münzdeutung zu behaupten. Karte als Nachrichtenkarte mit Personenkästchen, Käfer als generisches Insekt. Räumliche Seitenpositionen sind unsere eigenen.
- Zustand: `story` ist eine validierte D6-ID, `value` bleibt interner Seitenindex 1–6. Eigene Sets speichern pro Würfel die IDs in `stories`; unbekannte IDs/URLs und Story-IDs auf anderen Würfelformen werden nicht übernommen.
- UI: Set über die vorhandene Set-Auswahl, feste helle Körper und dunkle Motive. Gemischte eigene Sets bleiben möglich. Motivnamen in barrierefreien Labels; Summen zählen ausschließlich Zahlenwürfel. Kein neuer Würfel-/Textur-Editor.
- 3D: bestehende Geometrie, Physik und kryptographische Seitenrandomisierung unverändert. Atlas-Key enthält Story-ID und Permutation. Während des Ladens ein gemeinsamer leerer Atlas; nach sechs geladenen Bildern ein fertiger Atlas pro Würfel/Permutation.
- Offline: Modul, alle 54 SVGs und Lizenzdatei im Serviceworker-Cache `handydice-2d-3d-v13`.

**Ausgeführte Checks:** `npm test` — sieben Testdateien bestanden, einschließlich Speicher-/ID-Validierung und gemischter Summen. Browser-Smoke unter Chrome 155: Set-Auswahl, alle 54 SVGs als Kontaktbogen, neun 2D-Bildseiten mit passenden Motivnamen, Ablegen/Reload, gemischtes eigenes Set speichern und wieder auswählen, echter 3D-Wurf, 3D-Ablegen/Zurückholen und Wechsel zurück zu 2D mit identischen Seitenindizes.

Zusätzlicher Browser-Atlascheck: alle 54 Flächen mit Permutation `[6,3,2,5,4,1]` gegen erwartete Motive verglichen (kleine Antialiasing-Abweichungen berücksichtigt). Ein leerer Ladeatlas plus neun fertige Atlanten, wiederholte Abrufe ohne weitere Uploads; feste weiße Körper trotz übergebener Farb-/Skinvarianten; WebGL-Fehlercode 0.

WebGL-Context-Verlust/Wiederherstellung erhielt die gespeicherte Runde. Offline-Reload in 3D erfolgreich; alle 56 Story-Dateien im Cache vorhanden und alle 54 SVGs ohne Netzwerk decodiert. Die bisherigen Source-Text-Prüfungen für Übersetzungs-/Assetlisten wurden durch diese tatsächlichen Laufzeitprüfungen ersetzt; Sprachwahl-Verhaltenstests bleiben erhalten. Keine dauerhaften Smoke-Skripte oder Vorschau-Overlays.

**Offene Quellenfrage:** Für exakt heutige/geometrisch originalgetreue Replik fehlen Komplettbilder der aktuellen Classic-Auflage und Gegenüber-Paare. Für eigenes, am belegten Original inspiriertes Standardset keine technische Voraussetzung. Rechtslage öffentlich separat beurteilen; neue Icons allein sind keine Freigabe.
