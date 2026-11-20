# Une image du milieu de chaque plan

## Ce que fait CineMosaic

CineMosaic prend une image au milieu de chaque plan d’un montage et les dispose toutes sur une planche, plan après plan, de gauche à droite et de haut en bas. Vous voyez la planche pendant que vous la réglez et l’enregistrez en PNG, TIFF, JPEG ou PDF.

Il travaille à partir d’un chutier Media Composer et des Avid MediaFiles de vos volumes, ou d’un fichier master du montage et de sa liste de montage. CineMosaic ne fait que lire : il ne modifie jamais vos médias, chutiers ou projets.

## Ce qui compte comme plan

À chaque instant, c’est la piste vidéo la plus haute contenant un clip qui compte – l’image que vous voyez. Le remplissage laisse voir les pistes du dessous. Un clip partiellement couvert par un clip sur une piste plus haute donne deux plans : avant et après l’insert. Les titres et effets sur les pistes supérieures comptent comme image.

L’image retenue est l’image du milieu de chaque plan. Pour un plan de 50 images, c’est l’image 25.

## Fichier master et liste de montage

Choisissez Source › Fichier master + liste de montage. Le fichier master est le montage exporté (QuickTime, MXF et tout ce que lit ffmpeg). La liste de montage peut être une EDL (CMX3600), un XML Final Cut Pro 7 (Premiere Pro, DaVinci Resolve), un FCPXML (Final Cut Pro, Resolve) ou un AAF.

CineMosaic place chaque plan dans le master par son timecode : timecode du plan moins timecode de début du master. Si le master commence à 10:00:00:00, la liste de montage doit commencer là aussi. Si des plans se trouvent avant le début du master, CineMosaic l’indique sous la source.

Une EDL ne contient qu’une piste vidéo et pas de cadence ; CineMosaic prend la cadence du master.

## Chutier Media Composer

Choisissez Source › Chutier Media Composer, puis le chutier (.avb). Sous Séquence, Toutes les séquences (par défaut) prend chaque séquence du chutier l’une après l’autre, par ordre de nom (Reel 2 avant Reel 10) ; ou choisissez-en une seule. En dessous sont listés tous les volumes avec un dossier « Avid MediaFiles » ; désactivez ceux que CineMosaic ne doit pas parcourir. Les volumes connectés plus tard apparaissent d’eux-mêmes.

CineMosaic suit chaque plan à travers les effets, effets de vitesse, multicam et imbrications jusqu’à son fichier média. Les clips d’autres chutiers du projet sont trouvés aussi. Les fichiers liés en AMA (QuickTime et autres hors d’Avid MediaFiles) sont lus là où Media Composer les lie.

## Extraire les images

Cliquez Extraire les images. La barre montre l’avancement et le temps restant ; Pause et Annuler l’arrêtent à tout moment – Annuler ne garde rien. Une image illisible reste une case grise sur la planche ; la ligne sous la source nomme la première et la raison.

## Format d’image

Si votre image a des bandes noires, choisissez son format sous Planche › Format d’image : 4:3, 1.66, 16:9, 1.85, 2.00, 2.35, 2.39 ou le vôtre (par exemple 2.39 ou 21:9). CineMosaic coupe les bandes autour du centre de l’image – en haut et en bas pour les formats larges, à gauche et à droite pour les étroits. Image entière garde toute l’image. Vous pouvez changer le format sans extraire à nouveau.

## La planche

Papier : DIN A0 à A6 ou Personnalisé (largeur × hauteur en mm), portrait ou paysage. Résolution en dpi : 300 pour l’impression, 150 pour l’écran. Marge est le bord vide autour de tout, Espace la place entre deux images, les deux en mm. Avec Colonnes automatiques, CineMosaic choisit le nombre de colonnes qui rend les images les plus grandes ; désactivez-le pour fixer le nombre vous-même. La ligne en dessous montre la grille, la taille de la planche en pixels et celle d’une image.

Les très grandes planches (A0 à 600 dpi) demandent beaucoup de mémoire pour l’enregistrement ; la ligne sous la planche indique combien.

## Cadre, titre et traits de coupe

La colonne latérale a quatre onglets : Source, Planche, Cadre et Titre. Sous Cadre, l’option Cadre trace un trait autour de la planche : choisissez un motif (simple, double, épais – fin, fin – épais, fin – épais – fin, deux proches et un écarté), l’épaisseur d’un trait fin, la distance au bord et la couleur (noir au départ ; une pastille ou une valeur hexadécimale comme #3c4146).

Sous Titre, saisissez le titre. La liste des polices montre chaque police de votre Mac avec son propre dessin ; ↑ et ↓ la parcourent et la planche suit aussitôt, une lettre saute à la police suivante qui commence ainsi. Puis taille en points, gras, italique et couleur. L’ancre est le point de la planche auquel le titre est accroché – l’un des neuf, comme dans un logiciel d’image ; X et Y le déplacent depuis là en mm. Vous pouvez aussi faire glisser le titre dans l’aperçu. Là où il couvre le cadre, le trait s’interrompt autour de lui. En haut ou en bas, les images gardent leur distance ; au milieu, il passe par-dessus.

Planche › Traits de coupe met la planche en page dans une zone au format de l’image à l’intérieur du papier – pour des images en 2:1, une zone en 2:1 – et place des traits de coupe à ses coins pour la découper.

## Enregistrer

Choisissez le format et cliquez Enregistrer…. PNG, TIFF et JPEG contiennent la résolution, un programme d’impression les imprime donc à la taille choisie. La page PDF a la taille du papier. Afficher dans le Finder montre le fichier enregistré.

## Réglages et aide

CineMosaic › Réglages… (⌘,) contient Clair/Sombre et la langue (English, Deutsch, Français, Español). Mises à jour : CineMosaic cherche lui-même de nouvelles versions sur GitHub et affiche un avis ; Installer la mise à jour remplace l’app et la relance. Assistance : un autotest sur ses propres données, Signaler… pour les problèmes et idées (sans compte GitHub) et le fichier journal. Aide › Afficher le fichier journal le montre dans le Finder.

## Bon à savoir

CineMosaic lit les images avec FFmpeg. Les images sont gardées dans ~/Library/Caches/CineMosaic jusqu’à la prochaine extraction. Si macOS demande si CineMosaic peut accéder aux fichiers d’un volume amovible ou réseau, autorisez-le – tant que vous ne répondez pas, la lecture du volume attend.

Avid et Media Composer sont des marques d’Avid Technology, Inc. CineMosaic n’est pas affilié à Avid.
