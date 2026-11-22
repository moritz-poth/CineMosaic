# Une image du milieu de chaque plan

## Ce que fait CineMosaic

CineMosaic prend une image au milieu de chaque plan d’un montage et les dispose toutes sur une planche, plan après plan, de gauche à droite et de haut en bas. Vous voyez la planche pendant que vous la réglez et l’enregistrez en PNG, TIFF, JPEG ou PDF.

Il travaille à partir d’un chutier Media Composer et des Avid MediaFiles de vos volumes, ou d’un fichier master du montage et de sa liste de montage. CineMosaic ne fait que lire : il ne modifie jamais vos médias, chutiers ou projets.

## Ce qui compte comme plan

À chaque instant, c’est la piste vidéo la plus haute contenant un clip qui compte – l’image que vous voyez. Le remplissage laisse voir les pistes du dessous. Un clip partiellement couvert par un clip sur une piste plus haute donne deux plans : avant et après l’insert. Les titres et effets sur les pistes supérieures comptent comme image.

L’image retenue est l’image du milieu de chaque plan. Pour un plan de 50 images, c’est l’image 25. S’il reste des cases libres dans la dernière rangée, les plans les plus longs donnent deux images au lieu d’une – après le premier et après le deuxième tiers (pour 90 images : images 30 et 60) – jusqu’à ce que la planche soit pleine. Si vous modifiez la planche après la récupération et qu’elle demande des images absentes, elles restent grises et le bouton indique « Récupérer N images manquantes » : il ne récupère que celles-là.

## Fichier master et liste de montage

Choisissez Source › Fichier master + liste de montage. Ajoutez un ou plusieurs fichiers master et listes de montage (Ajouter… ; le × en retire un). Une liste de montage va avec le fichier master du même nom – Reel1.mov et Reel1.xml ; CineMosaic signale les fichiers sans partenaire et place les plans de toutes les paires l’un après l’autre. Le fichier master est le montage exporté (QuickTime, MXF et tout ce que lit ffmpeg). La liste de montage peut être une EDL (CMX3600), un XML Final Cut Pro 7 (Premiere Pro, DaVinci Resolve), un FCPXML (Final Cut Pro, Resolve) ou un AAF.

Pas de liste de montage ? Détecter les coupes (sous les listes de montage) trouve les coupes dans l’image de chaque fichier master sans liste ; il apparaît dans la liste comme « Reel1.mov – 12 plans détectés » (le × le retire). Tout le film est lu, ce qui prend à peu près le temps d’une lecture rapide – pause et annulation fonctionnent comme pour l’extraction. Les transitions douces (fondus) ne sont pas toujours trouvées ; une liste de montage est plus exacte. Le film commence à et Le film finit à (timecodes du fichier master, p. ex. 01:01:30:00) laissent de côté le générique de début et de fin : seuls les plans entre les deux sont pris, et un changement agit aussitôt, sans nouvelle analyse. Les champs fonctionnent comme les champs de timecode des logiciels de montage : un clic sur heures, minutes, secondes ou images les choisit, deux chiffres les remplacent (la partie suivante est alors choisie), ↑ et ↓ comptent, ← et → choisissent la voisine. Un double-clic, ⌘A ou Tab choisit tout le timecode : les chiffres le remplissent alors par la droite comme dans Media Composer (1013000 donne 01:01:30:00), Retour arrière vide le champ. ⌘V colle un timecode. Les projets gardent les deux. Vides, tout le film est analysé ; avec plusieurs fichiers master, ils comptent dans le timecode de chaque master. Pour les changer ensuite, retirez l’entrée détectée avec × et détectez à nouveau.

CineMosaic place chaque plan dans le master par son timecode : timecode du plan moins timecode de début du master. Si le master commence à 10:00:00:00, la liste de montage doit commencer là aussi. Si des plans se trouvent avant le début du master, CineMosaic l’indique sous la source.

Une EDL ne contient qu’une piste vidéo et pas de cadence ; CineMosaic prend la cadence du master.

## Chutier Media Composer

Choisissez Source › Chutier Media Composer, puis le chutier (.avb). Dans le menu Séquence, Toutes les séquences (par défaut) prend chaque séquence du chutier l’une après l’autre, par ordre de nom (Reel 2 avant Reel 10) ; ou choisissez des séquences – chacune reçoit une coche, donc une ou trois sur cinq. En dessous sont listés tous les volumes avec un dossier « Avid MediaFiles » ; désactivez ceux que CineMosaic ne doit pas parcourir. Les volumes connectés plus tard apparaissent d’eux-mêmes.

CineMosaic suit chaque plan à travers les effets, effets de vitesse, multicam et imbrications jusqu’à son fichier média. Les clips d’autres chutiers du projet sont trouvés aussi. Les fichiers liés en AMA (QuickTime et autres hors d’Avid MediaFiles) sont lus là où Media Composer les lie.

## Extraire les images

Cliquez Extraire les images. La barre montre l’avancement et le temps restant ; Pause et Annuler l’arrêtent à tout moment – Annuler ne garde rien. Une image illisible reste une case grise sur la planche ; la ligne sous la source nomme la première et la raison.

## Format d’image

Si votre image a des bandes noires, choisissez son format sous Planche › Format d’image – formats cinéma (4:3, 1.66:1, 16:9, 1.85:1, 2:1, 2.35:1, 2.39:1). Largeur : hauteur en dessous montre toujours le format dans un champ ; saisissez-y le vôtre (par exemple 2.39 : 1 ou 21 : 9). CineMosaic coupe les bandes autour du centre de l’image – en haut et en bas pour les formats larges, à gauche et à droite pour les étroits. Image entière garde toute l’image. Vous pouvez changer le format sans extraire à nouveau.

## La planche

Largeur × hauteur est toujours affiché : choisissez DIN A0 à A6 ou une taille réseaux sociaux dans la liste Papier pour le remplir, ou saisissez votre propre format – la liste indique alors Personnalisé (ou la taille correspondante). Les tailles réseaux sociaux sont en pixels : 1080 × 1920 (Reels, TikTok, Shorts, Stories), 1080 × 1350 et 1080 × 1440 (Instagram, Facebook), 1080 × 1080, 1000 × 1500 (Pinterest), 1200 × 628 (Facebook, LinkedIn) et 1920 × 1080 (YouTube, X) ; en choisir une passe l’unité en pixels. Portrait ou paysage tourne la planche. Unité fait passer la taille du papier des mm aux pixels ; marge, espace, cadre et X et Y du titre sont toujours en pixels de la planche (à 300 dpi, 118 px font 10 mm) ; la taille du texte reste en points. Toutes sont des nombres entiers. Résolution en dpi : 300 pour l’impression, 150 pour l’écran. Marge est le bord vide autour de tout, Espace la place entre deux images – appuyez dans le champ et faites glisser à gauche ou à droite pour changer la valeur, ou cliquez pour la saisir. Avec Colonnes automatiques, CineMosaic choisit le nombre de colonnes qui rend les images les plus grandes ; désactivez-le pour fixer le nombre vous-même. La ligne en dessous montre la grille et la taille d’une image en pixels.

Les très grandes planches (A0 à 600 dpi) demandent beaucoup de mémoire pour l’enregistrement ; la ligne sous la planche indique combien.

## Cadre, titre et traits de coupe

La colonne latérale a quatre onglets : Source, Planche, Cadre et Titre. Sous Cadre, l’option Cadre trace un trait autour de la planche : choisissez un motif (simple, double, épais – fin, fin – épais, fin – épais – fin, deux proches et un écarté), l’épaisseur d’un trait fin, la distance au bord et la couleur. Épaisseur et distance sont en pixels de la planche (à 300 dpi, 12 px font environ 1 mm) ; changer la résolution change leur taille sur le papier.

Sous Titre, saisissez le titre. Le menu des polices montre chaque police de votre Mac avec son propre dessin. Un clic ouvre la liste ; ↑ et ↓ parcourent les polices – liste ouverte ou fermée – et la planche suit aussitôt, une lettre saute à la police suivante qui commence ainsi, Retour ferme la liste. Puis taille en points, gras, italique et couleur. Projets et préréglages gardent la police par son nom ; sur un Mac qui ne l’a pas, CineMosaic nomme la police manquante et compose le titre en Helvetica Neue. L’ancre est le point de la planche auquel le titre est accroché – l’un des neuf, comme dans un logiciel d’image ; X et Y le déplacent depuis là en pixels de la planche. Vous pouvez aussi faire glisser le titre dans l’aperçu. Là où il couvre le cadre, le trait s’interrompt autour de lui. Espace en haut · en bas et Espace à gauche · à droite (pixels de la planche) est ce que les images laissent libre autour du titre : en haut, elles restent d’autant en dessous, en bas d’autant au-dessus. Au milieu, le titre passe par-dessus les images – sauf si Images autour du titre est activé : les cellules qu’il touche avec son espace restent vides et les images continuent dans les cellules suivantes ; la grille s’agrandit pour qu’elles tiennent toutes. De petits traits sur le cadre bleu autour du titre marquent le milieu du texte, en haut, en bas, à gauche et à droite. Activez Grille pour des lignes tous les N % de la page – ou de la zone à l’intérieur des traits de coupe – afin de placer le titre (10 % au départ, les lignes à 50 % plus marquées) ; les petits traits passent au vert quand le milieu du texte est exactement sur une ligne. En faisant glisser le titre, son milieu s’accroche à une ligne proche. La grille n’apparaît que dans l’aperçu, jamais à l’export. Tous les outils sont là avant même de saisir un titre.

Couleur, pour le cadre et le titre : les cinq pastilles rondes sont vos préréglages de couleur. Un clic prend leur couleur. Un clic dans le champ hexadécimal ouvre la fenêtre des couleurs de macOS : la couleur choisie devient le préréglage choisi, tout comme une valeur comme #3c4146 saisie dans le champ. Les préréglages restent dans CineMosaic pour chaque projet. Si la fenêtre est trop basse pour tout le contenu de la colonne latérale, celle-ci défile.

Planche › Traits de coupe met la planche en page dans une zone au format de l’image à l’intérieur du papier – pour des images en 2:1, une zone en 2:1 – et place des traits de coupe à ses coins pour la découper.

## Projets

Fichier › Enregistrer le projet (⌘S) garde tout dans un fichier de projet : source, planche, titre et images récupérées. Fichier › Ouvrir un projet… (⌘O), Ouvrir l’élément récent ou un double-clic dans le Finder le rouvre – aussitôt, avec ses images, même si le master, le chutier ou les volumes ne sont pas connectés. CineMosaic relit ensuite la source : si le montage a changé, les plans inchangés gardent leurs images, les nouveaux ou modifiés sont gris et « Récupérer N images manquantes » ne récupère que ceux-là (les images déjà récupérées sont réutilisées). Chaque projet a sa propre fenêtre : Nouveau projet (⌘N) ouvre une nouvelle fenêtre vide, et un projet ouvert reçoit aussi la sienne (une fenêtre vide le prend ; un projet déjà ouvert passe au premier plan). Enregistrer le projet sous… (⇧⌘S) en fait une copie, Fermer la fenêtre (⌘W) en ferme une. Avant de perdre des modifications non enregistrées – fermer une fenêtre ou Quitter – CineMosaic demande s’il faut les enregistrer.

## Préréglages

Un préréglage garde l'aspect de la planche : papier, orientation, résolution, marge, espacement, colonnes, format d'image, traits de coupe, cadre, police, taille, style, couleur et place du titre, et le format d'export. Il ne garde ni la source ni le texte du titre : ils appartiennent au projet. Les préréglages restent dans CineMosaic.

Ouvrez le menu **Préréglage** en bas de la colonne latérale :

- **Nouveau préréglage…** – tapez un nom et cliquez sur **Enregistrer** (Retour fonctionne aussi, Échap annule).
- Choisir un préréglage dans la liste règle tous les champs d'un coup.
- **Remplacer par les réglages actuels** – le préréglage choisi prend les champs tels qu'ils sont.
- **Renommer…** – donne un nouveau nom au préréglage choisi.
- **Supprimer…** – demande une fois, puis supprime le préréglage choisi.

La police est gardée par son nom : si ce Mac ne l'a pas, la police reste inchangée.

## Enregistrer

Choisissez le format et cliquez Enregistrer…. Si des images dont la planche a besoin ne sont pas encore récupérées (après un changement de format ou de planche), CineMosaic le signale d’abord : enregistrer quand même avec des cases grises, ou annuler et les récupérer. PNG, TIFF et JPEG contiennent la résolution, un programme d’impression les imprime donc à la taille choisie. La page PDF a la taille du papier. Afficher dans le Finder montre le fichier enregistré.

## Réglages et aide

CineMosaic › Réglages… (⌘,) contient Clair/Sombre et la langue (English, Deutsch, Français, Español). Mises à jour : CineMosaic cherche lui-même de nouvelles versions sur GitHub et affiche un avis ; Installer la mise à jour remplace l’app et la relance. Assistance : un autotest sur ses propres données, Signaler… pour les problèmes et idées (sans compte GitHub) et le fichier journal. Aide › Afficher le fichier journal le montre dans le Finder.

## Bon à savoir

CineMosaic lit les images avec FFmpeg. Les images sont gardées dans ~/Library/Caches/CineMosaic tant que l’app est ouverte et supprimées quand vous la quittez (les restes d’un plantage au démarrage suivant). Si macOS demande si CineMosaic peut accéder aux fichiers d’un volume amovible ou réseau, autorisez-le – tant que vous ne répondez pas, la lecture du volume attend.

Avid et Media Composer sont des marques d’Avid Technology, Inc. CineMosaic n’est pas affilié à Avid.
