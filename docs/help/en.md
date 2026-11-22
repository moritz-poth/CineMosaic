# One still from the middle of every shot

## What CineMosaic does

CineMosaic takes one still from the middle of every shot of a cut and lays all of them out on one sheet, shot after shot, left to right and top to bottom. You see the sheet as you set it up and save it as PNG, TIFF, JPEG or PDF.

It works from a Media Composer bin with the Avid MediaFiles of your volumes, or from a master file of the cut with its cut list. CineMosaic only reads: it never changes your media, bins or projects.

## What counts as a shot

At every moment the topmost video track that holds a clip counts – the picture you see. Filler lets the tracks below show through. A clip that is partly covered by a clip on a higher track becomes two shots: before and after the insert. Titles and effects on upper tracks count as picture.

The still is the middle frame of each shot. For a shot of 50 frames that is frame 25. If the grid has cells left over in its last row, the longest shots give two stills instead of one – after the first and after the second third (for 90 frames: frames 30 and 60) – until the sheet is full. If you change the sheet after grabbing and it needs stills that are not there yet, they show grey and the button reads "Grab N missing stills": it grabs only those.

## Master file and cut list

Choose Source › Master file + cut list. Add one or several master files and cut lists (Add…; the × removes one). A cut list belongs to the master file with the same name – Reel1.mov and Reel1.xml; CineMosaic names files without a partner, and lays the shots of all pairs one after the other. The master file is the exported cut (QuickTime, MXF and anything else ffmpeg reads). The cut list can be an EDL (CMX3600), a Final Cut Pro 7 XML (Premiere Pro, DaVinci Resolve), an FCPXML (Final Cut Pro, Resolve) or an AAF.

No cut list? Detect cuts (below the cut lists) finds the cuts in the picture of every master file without a cut list; it appears in the list as “Reel1.mov – 12 shots detected” (the × removes it). This reads the whole film and takes about as long as playing it through fast – pause and cancel work as when grabbing. Soft transitions (dissolves, fades) are not always found; a cut list is more exact. Film starts at and Film ends at (timecodes of the master file, e.g. 01:01:30:00) leave out opening and end titles: only the shots between them are used, and a change shows at once, without analysing again. The fields work like the timecode fields of editing apps: click hours, minutes, seconds or frames to choose them, type two digits (the next part is chosen then), ↑ and ↓ count, ← and → choose the neighbour. A double-click, ⌘A or Tab chooses the whole timecode: digits then fill in from the right as in Media Composer (1013000 gives 01:01:30:00), Backspace empties the field. ⌘V pastes a timecode. Projects keep both. Leave them empty for the whole film; with several master files they count in each master's own timecode. To change them afterwards, remove the detected entry with × and detect again.

CineMosaic places every shot in the master by its timecode: the timecode of the shot minus the master's start timecode. If the master starts at 10:00:00:00, the cut list must start there too. If shots lie before the master's start, CineMosaic says so under the source.

An EDL holds only one video track and no frame rate; CineMosaic takes the frame rate from the master.

## Media Composer bin

Choose Source › Media Composer bin, then the bin (.avb). In the Sequence menu, All sequences (the default) takes every sequence of the bin one after the other, in name order (Reel 2 before Reel 10); or choose single sequences – each one you choose gets a tick, so one or three of five. Below, every volume with an Avid MediaFiles folder is listed; switch off the ones CineMosaic should not search. Volumes you connect later appear by themselves.

CineMosaic follows every shot through effects, motion effects, multicam and nests down to its media file. Clips from other bins of the project are found as well. AMA-linked files (QuickTime and others outside Avid MediaFiles) are read from where Media Composer links them.

## Grabbing the stills

Click Grab stills. The bar shows how far it is and how long it will take; Pause and Cancel stop it at any time – Cancel keeps nothing. A still that cannot be read stays a grey field on the sheet, and the line below the source names the first one and why.

## Picture format

If your picture has black bars, choose its format under Sheet › Picture format – film formats (4:3, 1.66:1, 16:9, 1.85:1, 1.9:1, 2:1, 2.35:1, 2.39:1, 2.4:1). Width : height below always shows the format in one field; type your own there (for example 2.39 : 1 or 21 : 9). CineMosaic cuts the bars off around the middle of the picture – top and bottom for wide formats, left and right for narrow ones. Full frame keeps the whole picture. You can change the format without grabbing the stills again. Full frame names the format of the stills as the film world does: DCI Scope (2048 × 858, 4096 × 1716) or 1920 × 804 is 2.39:1, the DCI container (2048 × 1080) 1.9:1, 1998 × 1080 1.85:1.

## The sheet

Width × height is always shown: choose DIN A0 to A6 or a social media size in the Paper list to fill it in, or type your own size – the list then shows Custom (or the size it matches). The social media sizes are in pixels: 1080 × 1920 (Reels, TikTok, Shorts, Stories), 1080 × 1350 and 1080 × 1440 (Instagram, Facebook), 1080 × 1080, 1000 × 1500 (Pinterest), 1200 × 628 (Facebook, LinkedIn) and 1920 × 1080 (YouTube, X); choosing one switches the unit to pixels. Portrait or landscape turns the sheet. Unit switches the paper size between mm and pixels; margin, gap, the frame line and the headline's X and Y are always in pixels of the sheet (at 300 dpi, 118 px are 10 mm); the type size stays in points. All of them are whole numbers. Resolution in dpi: 300 for print, 150 for the screen. Margin is the empty border around everything; a negative margin runs the stills off the page so they fill it to the edges (what goes over is cut off), Gap the space between two stills – press in the field and drag left or right to change them, or click and type. With Columns automatic CineMosaic chooses the number of columns that makes the stills largest; switch it off to set the number yourself. The line below shows the grid and the size of one still in pixels.

Very large sheets (A0 at 600 dpi) need a lot of memory to save; the line below the sheet says how much.

## Frame line, headline and crop marks

The side column has four tabs: Source, Sheet, Frame line and Headline. Under Frame line, the switch draws a line around the sheet: choose a pattern (single, double, thick – thin, thin – thick, thin – thick – thin, two close and one apart), the thickness of a thin strand, the distance from the edge and the colour. Thickness and distance are in pixels of the sheet (at 300 dpi, 12 px are about 1 mm); a change of the resolution changes their size on paper.

Under Headline, type the title. The font menu shows every font of your Mac in its own look. Click it to open the list; ↑ and ↓ step through the fonts – open or closed – and the sheet follows at once, a letter jumps to the next font starting with it, Return closes the list. Then size in points, bold, italic and colour. Projects and presets keep the font by name; opened on a Mac without it, CineMosaic names the missing font and sets the headline in Helvetica Neue. The anchor is the point of the sheet the headline hangs at – one of nine, like in an image editor; X and Y move it from there in pixels of the sheet. You can also drag the headline in the preview. Where it covers the frame line, the line breaks around it. Space above · below and Space left · right (pixels of the sheet) is what the stills keep free around the headline: at the top they stay that far below it, at the bottom that far above it. In the middle the headline lies over the stills – unless Stills around the headline is on: then the cells it covers, with its space around it, stay empty and the stills go on in the next cells; the grid grows so that all of them still fit. Small ticks on the blue frame around the headline mark the middle of the text, top, bottom, left and right. Switch on Grid for lines every N % of the page – or of the area inside the crop marks – to place the headline (10 % at first, the 50 % lines stronger); the ticks turn green when the middle of the text lies exactly on a grid line. Dragging the headline, its middle snaps to a grid line it comes close to. The grid is only shown in the preview, never exported. All tools are there even before you type a headline.

Colour, for the frame line and the headline: the five round swatches are your colour presets. Click one to use its colour. A click into the hex field opens the macOS colour window: the colour you pick there becomes the chosen preset, and so does a hex value like #3c4146 typed into the field. The presets are kept in CineMosaic for every project. If the window is too low for everything in the side column, it scrolls.

Sheet › Crop marks lays the sheet out in an area of the picture format inside the paper – for stills in 2:1 an area of 2:1 – and puts crop marks at its corners, so you can cut it out.

## Projects

File › Save Project (⌘S) keeps everything as one project file: source, sheet, headline and the grabbed stills. File › Open Project… (⌘O), Open Recent or a double-click in the Finder brings it back – at once, with its stills, even when the master, the bin or the volumes are not connected. CineMosaic then reads the source again: if the cut has changed, unchanged shots keep their stills, new or changed ones show grey and "Grab N missing stills" grabs only those (frames grabbed before are reused). Every project has its own window: New Project (⌘N) opens a new, empty window, and an opened project gets its own window too (an empty window takes it; a project that is already open comes to the front). Save Project As… (⇧⌘S) makes a copy, Close Window (⌘W) closes one. Before unsaved changes would be lost – closing a window or Quit – CineMosaic asks whether to save them.

## Presets

A preset keeps the look of the sheet: paper, orientation, resolution, margin, gap, columns, picture format, crop marks, frame line, the headline's font, size, style, colour and place, and the export format. It does not keep the source or the headline text – they belong to the project. Presets stay inside CineMosaic.

Open the **Preset** menu at the bottom of the side column:

- **New preset…** – type a name and click **Save** (Return works too, Esc cancels).
- Choose a preset from the list and every field is set at once.
- **Update with current settings** – the chosen preset takes the fields as they are now.
- **Rename…** – gives the chosen preset a new name.
- **Delete…** – asks once, then removes the chosen preset.

The font is kept by name: if this Mac does not have it, the font stays as it is.

## Saving

Choose the format and click Save…. If stills the sheet needs are not grabbed yet (after a change of format or sheet), CineMosaic says so first: save anyway with grey cells, or cancel and grab them. PNG, TIFF and JPEG carry the resolution, so a print program prints them at the chosen size. The PDF page has the paper size. Show in Finder shows the saved file.

## Settings and help

CineMosaic › Settings… (⌘,) has Light/Dark and the language (English, Deutsch, Français, Español). Updates: CineMosaic looks for new versions on GitHub by itself and shows a notice; Install update replaces the app and starts it again. Support: a self-test on test data of its own, Report… for problems and ideas (no GitHub account needed) and the log file. Help › Show Log File shows the log file in Finder.

## Good to know

CineMosaic reads the frames with FFmpeg. The stills are kept in ~/Library/Caches/CineMosaic while the app is open and deleted when you quit it (left-overs of a crash at the next start). When macOS asks whether CineMosaic may access files on a removable or network volume, allow it – until you answer, reading the volume waits.

Avid and Media Composer are trademarks of Avid Technology, Inc. CineMosaic is not affiliated with Avid.
