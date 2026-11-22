<h1 align="center"><img src="assets/icon.png" width="72" align="absmiddle" alt="">&nbsp;&nbsp;CINEMOSAIC</h1>

<p align="center"><code>Mac app</code>&nbsp;&nbsp;<code>macOS 11+</code>&nbsp;&nbsp;<code>free · GPL-3.0</code></p>

<p align="center">☕ If you use CineMosaic and like it, I would be glad if you <a href="https://buymeacoffee.com/moritzpoth">bought me a coffee</a>.</p>

For every clip in the timeline CineMosaic takes a still and sets them one after the other. What comes out is a small work of art of its own – a poster, a social media post or the cover of a premiere invitation.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/app-dark.jpg">
  <img src="docs/images/app-light.jpg" alt="CineMosaic with a poster of Tears of Steel: settings on the left, the sheet on the right">
</picture>

## Examples

All three from the same film – 132 shots, cuts found by CineMosaic in the picture, opening and end titles left out.

<p align="center">
  <img src="docs/images/example-poster.jpg" width="32%" alt="Poster, DIN A2 portrait, headline at the top, frame line">
  <img src="docs/images/example-instagram.jpg" width="32%" alt="Instagram post 1080 × 1350, the stills flow around the title in the middle and fill the page">
</p>
<p align="center">
  <img src="docs/images/example-invitation.jpg" width="65%" alt="Premiere invitation, DIN A6 landscape, triple frame line, title at the bottom">
</p>

Poster (DIN A2) · Instagram post (1080 × 1350) · cover of a premiere invitation (DIN A6).
Stills from *Tears of Steel* – (CC) Blender Foundation | [mango.blender.org](https://mango.blender.org), licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/).

## What it does

- **Two sources.** A Media Composer bin with the Avid MediaFiles of your volumes – CineMosaic follows every shot of the sequence through effects, motion effects, multicam and nests to its media file, also into other bins of the project and to AMA-linked files. Or a master file of the cut with its cut list: EDL (CMX3600), Final Cut Pro 7 XML, FCPXML or AAF.
- **What you see.** At every moment the topmost video track with a clip counts; filler lets the tracks below show through. A clip partly covered from above becomes two shots. The still is the middle frame of each shot. Cells left over in the last row are filled with a second still from the longest shots (after the first and the second third).
- **No cut list?** CineMosaic can find the cuts in the picture of the master file. Film start and end timecodes leave out opening and end titles – change them any time, the shots follow at once.
- **Picture format.** Black bars are cut off for 4:3, 1.66, 16:9, 1.85, 1.90, 2:1, 2.35, 2.39, 2.40 or any format of your own. The stills' own format is named as the film world does (DCI Scope 2048 × 858 is 2.39:1).
- **The sheet.** DIN A0–A6, social media sizes or your own size, portrait or landscape, resolution in dpi; margin (also negative, to fill the page to its edges) and gap in pixels; columns automatic (largest stills) or fixed; crop marks. A live preview shows the sheet as you set it up.
- **Frame line and headline.** A frame line in six patterns, a headline in any font of your Mac at one of nine anchors, dragged in the preview with a grid to place it exactly; the stills keep a space of your choice around it or flow around it. Five colour presets each, picked with the macOS colour window.
- **Presets.** Keep the look of a sheet and use it again for the next cut.
- **Saving.** PNG, TIFF and JPEG with the resolution in the file, PDF in the paper size.
- **Projects.** Save the whole sheet with its stills as a project, open it again later – also without the source – and when the cut has changed, grab only the new or changed shots.

CineMosaic only reads: it never changes media, bins or projects. English, German, French and Spanish, dark or light, a built-in manual (Help → CineMosaic Manual), updates from within the app.

Bins are read with a small reader of our own (`src/avb.rs`), AAF files with `src/aaf.rs`; both are based on the format knowledge of [pyavb](https://github.com/markreidvfx/pyavb) and [pyaaf2](https://github.com/markreidvfx/pyaaf2) (MIT, see THIRD-PARTY.md). Frames are read with [FFmpeg](https://ffmpeg.org).

## Install

Requires macOS 11 or later, Apple Silicon or Intel. Download `CineMosaic-<version>.dmg` from [Releases](../../releases), open it and drag CineMosaic onto Applications. It is not signed with a paid Apple certificate: on first launch open **System Settings → Privacy & Security** and click **Open Anyway** (once). When macOS asks whether CineMosaic may access files on removable or network volumes, allow it.

## Build

```
cargo test
scripts/build-mac-app.sh     # → dist/CineMosaic.app (universal), the zip for the updater and the DMG
cargo run --release --bin cinemosaic -- MASTER CUTLIST OUT.pdf [--paper A3 --landscape --dpi 300 --picture 2.39]
cargo run --release --bin cinemosaic -- --bin BIN.avb VOLUME[:VOLUME] OUT.png [--seq N]
```

Static `ffmpeg` and `ffprobe` for both architectures go into `vendor/ffmpeg/arm64/` and `vendor/ffmpeg/x86_64/`; the build script puts them into the app.

## License

Copyright © 2026 Moritz Poth. Free software under the GNU General Public License v3.0 or later – see [LICENSE](LICENSE). No warranty. Font: [JetBrains Mono](https://www.jetbrains.com/lp/mono/) (SIL Open Font License, `ui/fonts/OFL.txt`).

*Avid and Media Composer are trademarks of Avid Technology, Inc. This project is not affiliated with Avid.*
