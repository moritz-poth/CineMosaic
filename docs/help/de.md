# Ein Still aus der Mitte jedes Shots

## Was CineMosaic macht

CineMosaic holt aus der Mitte jedes Shots eines Schnitts ein Still und ordnet alle auf einem Bogen an, Shot für Shot, von links nach rechts und von oben nach unten. Du siehst den Bogen, während du ihn einrichtest, und speicherst ihn als PNG, TIFF, JPEG oder PDF.

Es arbeitet mit einem Media-Composer-Bin und den Avid MediaFiles deiner Volumes, oder mit einem Masterfile des Schnitts und seiner Schnittliste. CineMosaic liest nur: Es verändert nie deine Medien, Bins oder Projekte.

## Was als Shot zählt

In jedem Moment zählt die oberste Videospur, auf der ein Clip liegt – also das Bild, das du siehst. Füller ist durchsichtig, darunter liegende Spuren scheinen durch. Ein Clip, der teilweise von einem Clip auf einer höheren Spur verdeckt wird, ergibt zwei Shots: vor und nach dem Insert. Titel und Effekte auf oberen Spuren zählen als Bild.

Das Still ist der mittlere Frame jedes Shots. Bei einem Shot von 50 Frames ist das Frame 25. Bleiben in der letzten Reihe Felder frei, geben die längsten Shots statt einem zwei Stills – nach dem ersten und nach dem zweiten Drittel (bei 90 Frames: Frame 30 und 60) –, bis der Bogen voll ist. Änderst du den Bogen nach dem Holen und er braucht Stills, die noch fehlen, sind sie grau und der Knopf heißt „N fehlende Stills holen“: Er holt nur diese.

## Masterfile und Schnittliste

Wähle Quelle › Masterfile + Schnittliste. Das Masterfile ist der exportierte Schnitt (QuickTime, MXF und alles, was ffmpeg liest). Die Schnittliste kann eine EDL (CMX3600), ein Final-Cut-Pro-7-XML (Premiere Pro, DaVinci Resolve), ein FCPXML (Final Cut Pro, Resolve) oder ein AAF sein.

CineMosaic findet jeden Shot im Master über seinen Timecode: Timecode des Shots minus Start-Timecode des Masters. Beginnt der Master bei 10:00:00:00, muss auch die Schnittliste dort beginnen. Liegen Shots vor dem Start des Masters, steht das unter der Quelle.

Eine EDL enthält nur eine Videospur und keine Framerate; CineMosaic nimmt die Framerate des Masters.

## Media-Composer-Bin

Wähle Quelle › Media-Composer-Bin, dann den Bin (.avb). Unter Sequenz nimmt Alle Sequenzen (Standard) jede Sequenz des Bins nacheinander, nach Namen sortiert (Reel 2 vor Reel 10); oder wähle eine einzelne. Darunter stehen alle Volumes mit einem Ordner „Avid MediaFiles“; schalte die aus, die CineMosaic nicht durchsuchen soll. Volumes, die du später anschließt, erscheinen von selbst.

CineMosaic verfolgt jeden Shot durch Effekte, Motion-Effekte, Multicam und Nests bis zu seiner Mediendatei. Clips aus anderen Bins des Projekts werden auch gefunden. AMA-verlinkte Dateien (QuickTime und andere außerhalb von Avid MediaFiles) werden dort gelesen, wo Media Composer sie verlinkt.

## Stills holen

Klicke Stills holen. Der Balken zeigt, wie weit es ist und wie lange es noch dauert; Pause und Abbrechen halten jederzeit an – Abbrechen behält nichts. Ein Still, das sich nicht lesen lässt, bleibt ein graues Feld auf dem Bogen; die Zeile unter der Quelle nennt das erste und den Grund.

## Bildformat

Hat dein Bild schwarze Balken, wähle sein Format unter Bogen › Bildformat: 4:3, 1.66, 16:9, 1.85, 2.00, 2.35, 2.39 oder ein eigenes (zum Beispiel 2.39 oder 21:9). CineMosaic schneidet die Balken um die Bildmitte ab – oben und unten bei breiten Formaten, links und rechts bei schmalen. Vollbild behält das ganze Bild. Du kannst das Format ändern, ohne die Stills neu zu holen.

## Der Bogen

Papier: DIN A0 bis A6 oder Eigenes (Breite × Höhe in mm), Hoch- oder Querformat. Auflösung in dpi: 300 für den Druck, 150 für den Bildschirm. Rand ist der leere Rand um alles, Abstand der Platz zwischen zwei Stills, beides in mm – ins Feld drücken und nach links oder rechts ziehen ändert den Wert, Klicken öffnet es zum Tippen. Mit Spalten automatisch wählt CineMosaic die Spaltenzahl, bei der die Stills am größten werden; schalte es aus, um die Zahl selbst festzulegen. Die Zeile darunter zeigt das Raster, die Größe des Bogens in Pixeln und die Größe eines Stills.

Sehr große Bögen (A0 mit 600 dpi) brauchen zum Speichern viel Arbeitsspeicher; die Zeile unter dem Bogen sagt, wie viel.

## Rahmenlinie, Headline und Schnittmarken

Die Seitenspalte hat vier Reiter: Quelle, Bogen, Linie und Headline. Unter Linie zieht Rahmenlinie eine Linie um den Bogen: wähle ein Muster (einfach, doppelt, dick – dünn, dünn – dick, dünn – dick – dünn, zwei nah und eine weiter), die Dicke einer dünnen Linie, den Abstand zum Rand und die Farbe (zuerst Schwarz; ein Farbfeld oder ein Hex-Wert wie #3c4146).

Unter Headline tippst du den Titel. Die Schriftliste zeigt jede Schrift deines Macs in ihrem eigenen Aussehen; ↑ und ↓ blättern durch, der Bogen folgt sofort, ein Buchstabe springt zur nächsten Schrift mit diesem Anfang. Dann Größe in Punkt, fett, kursiv und Farbe. Der Anker ist der Punkt des Bogens, an dem die Headline hängt – einer von neun, wie in einem Bildprogramm; X und Y verschieben sie von dort in mm. Du kannst die Headline auch in der Vorschau ziehen. Wo sie die Rahmenlinie überdeckt, ist die Linie um sie herum unterbrochen. Oben oder unten halten die Stills Abstand; in der Mitte liegt sie über ihnen.

Bogen › Schnittmarken legt den Bogen in einer Fläche im Bildformat innerhalb des Papiers an – bei Stills in 2:1 eine Fläche in 2:1 – und setzt Schnittmarken an ihre Ecken, damit du sie ausschneiden kannst.

## Presets

Ein Preset merkt sich das Aussehen des Bogens: Papier, Ausrichtung, Auflösung, Rand, Abstand, Spalten, Bildformat, Schnittmarken, Rahmenlinie, Schrift, Größe, Stil, Farbe und Platz der Headline sowie das Exportformat. Die Quelle und den Headline-Text merkt es sich nicht – sie gehören zum Projekt. Presets bleiben in CineMosaic.

Öffne das Menü **Preset** unten in der Seitenspalte:

- **Neues Preset…** – Namen eintippen und **Sichern** klicken (Return geht auch, Esc bricht ab).
- Ein Preset aus der Liste wählen setzt alle Felder auf einmal.
- **Mit aktuellen Einstellungen überschreiben** – das gewählte Preset übernimmt die Felder, wie sie jetzt sind.
- **Umbenennen…** – gibt dem gewählten Preset einen neuen Namen.
- **Löschen…** – fragt einmal nach und entfernt dann das gewählte Preset.

Die Schrift wird über ihren Namen gemerkt: Fehlt sie auf diesem Mac, bleibt die Schrift, wie sie ist.

## Speichern

Wähle das Format und klicke Speichern…. PNG, TIFF und JPEG enthalten die Auflösung, ein Druckprogramm druckt sie also in der gewählten Größe. Die PDF-Seite hat die Papiergröße. Im Finder zeigen zeigt die gespeicherte Datei.

## Einstellungen und Hilfe

CineMosaic › Einstellungen… (⌘,) enthält Hell/Dunkel und die Sprache (English, Deutsch, Français, Español). Updates: CineMosaic sucht selbst auf GitHub nach neuen Versionen und zeigt einen Hinweis; Update installieren ersetzt die App und startet sie neu. Support: ein Selbsttest mit eigenen Testdaten, Melden… für Probleme und Ideen (kein GitHub-Konto nötig) und die Logdatei. Hilfe › Logdatei zeigen zeigt die Logdatei im Finder.

## Gut zu wissen

CineMosaic liest die Frames mit FFmpeg. Die Stills liegen in ~/Library/Caches/CineMosaic, solange die App offen ist, und werden beim Beenden gelöscht (Reste nach einem Absturz beim nächsten Start). Fragt macOS, ob CineMosaic auf Dateien eines Wechsel- oder Netzwerk-Volumes zugreifen darf, erlaube es – bis du antwortest, wartet das Lesen des Volumes.

Avid und Media Composer sind Marken von Avid Technology, Inc. CineMosaic steht in keiner Verbindung zu Avid.
