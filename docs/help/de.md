# Ein Still aus der Mitte jedes Shots

## Was CineMosaic macht

CineMosaic holt aus der Mitte jedes Shots eines Schnitts ein Still und ordnet alle auf einem Bogen an, Shot für Shot, von links nach rechts und von oben nach unten. Du siehst den Bogen, während du ihn einrichtest, und speicherst ihn als PNG, TIFF, JPEG oder PDF.

Es arbeitet mit einem Media-Composer-Bin und den Avid MediaFiles deiner Volumes, oder mit einem Masterfile des Schnitts und seiner Schnittliste. CineMosaic liest nur: Es verändert nie deine Medien, Bins oder Projekte.

## Was als Shot zählt

In jedem Moment zählt die oberste Videospur, auf der ein Clip liegt – also das Bild, das du siehst. Füller ist durchsichtig, darunter liegende Spuren scheinen durch. Ein Clip, der teilweise von einem Clip auf einer höheren Spur verdeckt wird, ergibt zwei Shots: vor und nach dem Insert. Titel und Effekte auf oberen Spuren zählen als Bild.

Das Still ist der mittlere Frame jedes Shots. Bei einem Shot von 50 Frames ist das Frame 25.

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

Papier: DIN A0 bis A6 oder Eigenes (Breite × Höhe in mm), Hoch- oder Querformat. Auflösung in dpi: 300 für den Druck, 150 für den Bildschirm. Rand ist der leere Rand um alles, Abstand der Platz zwischen zwei Stills, beides in mm. Mit Spalten automatisch wählt CineMosaic die Spaltenzahl, bei der die Stills am größten werden; schalte es aus, um die Zahl selbst festzulegen. Die Zeile darunter zeigt das Raster, die Größe des Bogens in Pixeln und die Größe eines Stills.

Sehr große Bögen (A0 mit 600 dpi) brauchen zum Speichern viel Arbeitsspeicher; die Zeile unter dem Bogen sagt, wie viel.

## Rahmenlinie, Headline und Schnittmarken

Die Seitenspalte hat drei Reiter: Quelle, Bogen und Gestaltung. Unter Gestaltung zieht Rahmenlinie eine Linie um den Bogen; stelle Dicke, Abstand zum Rand und Farbe ein (zuerst Schwarz; ein Farbfeld oder ein Hex-Wert wie #3c4146). Headline setzt einen Titel: Schrift aus den Schriften deines Macs, Größe in Punkt, fett, kursiv, Farbe und Position (oben oder unten, links, Mitte oder rechts). Mit Rahmenlinie sitzt die Headline auf der Linie, und die Linie ist um sie herum unterbrochen. Die Stills halten immer Abstand zu Linie und Headline.

Bogen › Schnittmarken legt den Bogen in einer Fläche im Bildformat innerhalb des Papiers an – bei Stills in 2:1 eine Fläche in 2:1 – und setzt Schnittmarken an ihre Ecken, damit du sie ausschneiden kannst.

## Speichern

Wähle das Format und klicke Speichern…. PNG, TIFF und JPEG enthalten die Auflösung, ein Druckprogramm druckt sie also in der gewählten Größe. Die PDF-Seite hat die Papiergröße. Im Finder zeigen zeigt die gespeicherte Datei.

## Einstellungen und Hilfe

CineMosaic › Einstellungen… (⌘,) enthält Hell/Dunkel und die Sprache (English, Deutsch, Français, Español). Updates: CineMosaic sucht selbst auf GitHub nach neuen Versionen und zeigt einen Hinweis; Update installieren ersetzt die App und startet sie neu. Support: ein Selbsttest mit eigenen Testdaten, Melden… für Probleme und Ideen (kein GitHub-Konto nötig) und die Logdatei. Hilfe › Logdatei zeigen zeigt die Logdatei im Finder.

## Gut zu wissen

CineMosaic liest die Frames mit FFmpeg. Die Stills liegen bis zum nächsten Holen in ~/Library/Caches/CineMosaic. Fragt macOS, ob CineMosaic auf Dateien eines Wechsel- oder Netzwerk-Volumes zugreifen darf, erlaube es – bis du antwortest, wartet das Lesen des Volumes.

Avid und Media Composer sind Marken von Avid Technology, Inc. CineMosaic steht in keiner Verbindung zu Avid.
