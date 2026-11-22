# Un fotograma del centro de cada plano

## Qué hace CineMosaic

CineMosaic toma un fotograma del centro de cada plano de un montaje y los coloca todos en una hoja, plano tras plano, de izquierda a derecha y de arriba abajo. Ves la hoja mientras la configuras y la guardas como PNG, TIFF, JPEG o PDF.

Trabaja con un bin de Media Composer y las Avid MediaFiles de tus volúmenes, o con un máster del montaje y su lista de montaje. CineMosaic solo lee: nunca cambia tus medios, bins ni proyectos.

## Qué cuenta como plano

En cada momento cuenta la pista de vídeo más alta que tiene un clip: la imagen que ves. El relleno deja ver las pistas de abajo. Un clip cubierto en parte por un clip de una pista más alta da dos planos: antes y después del inserto. Los títulos y efectos en pistas superiores cuentan como imagen.

El fotograma es el del centro de cada plano. En un plano de 50 fotogramas es el fotograma 25. Si quedan celdas libres en la última fila, los planos más largos dan dos fotogramas en lugar de uno – tras el primer y tras el segundo tercio (con 90 fotogramas: los fotogramas 30 y 60) – hasta llenar la hoja. Si cambias la hoja después de capturar y necesita fotogramas que aún faltan, aparecen en gris y el botón dice «Capturar N fotogramas que faltan»: captura solo esos.

## Máster y lista de montaje

Elige Origen › Máster + lista de montaje. Añade uno o varios archivos máster y listas de montaje (Añadir…; la × quita uno). Una lista de montaje va con el máster del mismo nombre: Reel1.mov y Reel1.xml; CineMosaic indica los archivos sin pareja y coloca los planos de todas las parejas uno tras otro. El máster es el montaje exportado (QuickTime, MXF y todo lo que lee ffmpeg). La lista de montaje puede ser una EDL (CMX3600), un XML de Final Cut Pro 7 (Premiere Pro, DaVinci Resolve), un FCPXML (Final Cut Pro, Resolve) o un AAF.

¿Sin lista de montaje? Detectar cortes (bajo las listas de montaje) encuentra los cortes en la imagen de cada máster sin lista; aparece en la lista como «Reel1.mov – 12 planos detectados» (la × lo quita). Se lee toda la película, lo que tarda más o menos como reproducirla rápido; pausa y cancelar funcionan como al extraer. Las transiciones suaves (fundidos) no siempre se encuentran; una lista de montaje es más exacta. La película empieza en y La película termina en (timecodes del máster, p. ej. 01:01:30:00) dejan fuera los créditos iniciales y finales: solo se toman los planos entre ambos, y un cambio se ve al momento, sin analizar de nuevo. Los campos funcionan como los de timecode de los programas de montaje: un clic en horas, minutos, segundos o fotogramas los elige, dos dígitos los sustituyen (después queda elegida la parte siguiente), ↑ y ↓ cuentan, ← y → eligen la vecina. Doble clic, ⌘A o Tab eligen todo el timecode: los dígitos lo rellenan entonces desde la derecha como en Media Composer (1013000 da 01:01:30:00), Retroceso vacía el campo. ⌘V pega un timecode. Los proyectos guardan ambos. Vacíos, se analiza toda la película; con varios másters cuentan en el timecode de cada uno. Para cambiarlos después, quita la entrada detectada con la × y vuelve a detectar.

CineMosaic sitúa cada plano en el máster por su timecode: timecode del plano menos timecode inicial del máster. Si el máster empieza en 10:00:00:00, la lista de montaje también debe empezar ahí. Si hay planos antes del inicio del máster, CineMosaic lo indica bajo el origen.

Una EDL solo tiene una pista de vídeo y no tiene cadencia; CineMosaic toma la del máster.

## Bin de Media Composer

Elige Origen › Bin de Media Composer, después el bin (.avb). En el menú Secuencia, Todas las secuencias (por defecto) toma cada secuencia del bin una tras otra, por orden de nombre (Reel 2 antes de Reel 10); o elige secuencias sueltas: cada una elegida lleva una marca, así una o tres de cinco. Debajo aparecen todos los volúmenes con una carpeta «Avid MediaFiles»; desactiva los que CineMosaic no debe buscar. Los volúmenes que conectes después aparecen solos.

CineMosaic sigue cada plano a través de efectos, efectos de velocidad, multicámara y anidados hasta su archivo de medios. También encuentra clips de otros bins del proyecto. Los archivos vinculados con AMA (QuickTime y otros fuera de Avid MediaFiles) se leen donde Media Composer los vincula.

## Extraer los fotogramas

Haz clic en Extraer fotogramas. La barra muestra el avance y el tiempo restante; Pausa y Cancelar lo detienen en cualquier momento; Cancelar no guarda nada. Un fotograma que no se puede leer queda como un recuadro gris en la hoja; la línea bajo el origen nombra el primero y el motivo.

## Formato de imagen

Si tu imagen tiene barras negras, elige su formato en Hoja › Formato de imagen: formatos de cine (4:3, 1.66:1, 16:9, 1.85:1, 1.9:1, 2:1, 2.35:1, 2.39:1, 2.4:1). Ancho : alto debajo muestra siempre el formato en un campo; escribe ahí uno propio (por ejemplo 2.39 : 1 o 21 : 9). CineMosaic recorta las barras alrededor del centro de la imagen: arriba y abajo en formatos anchos, a izquierda y derecha en estrechos. Fotograma completo conserva toda la imagen. Puedes cambiar el formato sin volver a extraer. Imagen completa nombra el formato de los fotogramas como el mundo del cine: DCI Scope (2048 × 858, 4096 × 1716) o 1920 × 804 es 2.39:1, el contenedor DCI (2048 × 1080) 1.9:1, 1998 × 1080 1.85:1.

## La hoja

Ancho × alto siempre está a la vista: elige DIN A0 a A6 o un tamaño de redes sociales en la lista Papel para rellenarlo, o escribe tu propio tamaño; la lista muestra entonces Personalizado (o el tamaño que coincide). Los tamaños de redes sociales están en píxeles: 1080 × 1920 (Reels, TikTok, Shorts, Stories), 1080 × 1350 y 1080 × 1440 (Instagram, Facebook), 1080 × 1080, 1000 × 1500 (Pinterest), 1200 × 628 (Facebook, LinkedIn) y 1920 × 1080 (YouTube, X); al elegir uno, la unidad pasa a píxeles. Vertical u horizontal gira la hoja. Unidad cambia el tamaño del papel entre mm y píxeles; margen, espacio, línea de marco y X e Y del título van siempre en píxeles de la hoja (a 300 dpi, 118 px son 10 mm); el tamaño de letra sigue en puntos. Todas son números enteros. Resolución en dpi: 300 para imprimir, 150 para la pantalla. Margen es el borde vacío alrededor de todo; un margen negativo hace que los fotogramas salgan de la página y la llenen hasta los bordes (lo que sobra se corta), Espacio el hueco entre dos fotogramas: pulsa en el campo y arrastra a izquierda o derecha para cambiar el valor, o haz clic para escribirlo. Con Columnas automáticas, CineMosaic elige el número de columnas que hace los fotogramas más grandes; desactívalo para fijar el número tú. La línea de abajo muestra la cuadrícula y el tamaño de un fotograma en píxeles.

Las hojas muy grandes (A0 a 600 dpi) necesitan mucha memoria para guardarse; la línea bajo la hoja dice cuánta.

## Línea de marco, título y marcas de corte

La columna lateral tiene cuatro pestañas: Origen, Hoja, Línea de marco y Título. En Línea de marco, el interruptor dibuja una línea alrededor de la hoja: elige un patrón (simple, doble, grueso – fino, fino – grueso, fino – grueso – fino, dos juntas y una aparte), el grosor de una línea fina, la distancia al borde y el color. Grosor y distancia van en píxeles de la hoja (a 300 dpi, 12 px son aprox. 1 mm); al cambiar la resolución cambia su tamaño en papel.

En Título escribe el título. El menú de fuentes muestra cada fuente de tu Mac con su propio aspecto. Un clic abre la lista; ↑ y ↓ recorren las fuentes – abierta o cerrada – y la hoja cambia al momento, una letra salta a la siguiente fuente que empieza así, Retorno cierra la lista. Después tamaño en puntos, negrita, cursiva y color. Proyectos y preajustes guardan la fuente por su nombre; en un Mac sin ella, CineMosaic nombra la fuente que falta y compone el título en Helvetica Neue. El ancla es el punto de la hoja del que cuelga el título, uno de nueve, como en un programa de imagen; X e Y lo mueven desde ahí en píxeles de la hoja. También puedes arrastrar el título en la vista previa. Donde cubre la línea de marco, la línea se interrumpe a su alrededor. Espacio arriba · abajo y Espacio izquierda · derecha (píxeles de la hoja) es lo que los fotogramas dejan libre alrededor del título: arriba quedan así de lejos por debajo, abajo así de lejos por encima. En el centro el título queda sobre los fotogramas, salvo con Fotogramas alrededor del título: entonces las celdas que toca con su espacio quedan vacías y los fotogramas siguen en las celdas siguientes; la cuadrícula crece para que quepan todos. Pequeñas marcas en el marco azul alrededor del título señalan el centro del texto arriba, abajo, a la izquierda y a la derecha. Activa Cuadrícula para ver líneas cada N % de la página – o de la zona dentro de las marcas de corte – y colocar el título (10 % al principio, las de 50 % más marcadas); las marcas se vuelven verdes cuando el centro del texto está justo sobre una línea. Al arrastrar el título, su centro se ajusta a una línea cercana. La cuadrícula solo se ve en la vista previa, nunca se exporta. Todas las herramientas están ahí antes de escribir un título.

Color, para la línea de marco y el título: las cinco muestras redondas son tus preajustes de color. Un clic toma su color. Un clic en el campo hexadecimal abre la ventana de colores de macOS: el color que elijas allí pasa a ser el preajuste elegido, igual que un valor como #3c4146 escrito en el campo. Los preajustes se guardan en CineMosaic para cada proyecto. Si la ventana es demasiado baja para todo lo de la columna lateral, esta se desplaza.

Hoja › Marcas de corte compone la hoja en una zona con el formato de imagen dentro del papel –para fotogramas en 2:1, una zona en 2:1– y pone marcas de corte en sus esquinas para recortarla.

## Proyectos

Archivo › Guardar proyecto (⌘S) guarda todo en un archivo de proyecto: origen, hoja, título y los fotogramas capturados. Archivo › Abrir proyecto… (⌘O), Abrir reciente o un doble clic en el Finder lo recupera, al momento y con sus fotogramas, aunque el máster, el bin o los volúmenes no estén conectados. Después CineMosaic vuelve a leer el origen: si el montaje ha cambiado, los planos sin cambios conservan sus fotogramas, los nuevos o modificados aparecen en gris y «Capturar N fotogramas que faltan» captura solo esos (los fotogramas ya capturados se reutilizan). Cada proyecto tiene su propia ventana: Nuevo proyecto (⌘N) abre una ventana nueva y vacía, y un proyecto abierto también recibe la suya (una ventana vacía lo toma; un proyecto ya abierto pasa al frente). Guardar proyecto como… (⇧⌘S) hace una copia, Cerrar ventana (⌘W) cierra una. Antes de perder cambios sin guardar – cerrar una ventana o Salir – CineMosaic pregunta si quieres guardarlos.

## Preajustes

Un preajuste guarda el aspecto de la hoja: papel, orientación, resolución, margen, separación, columnas, formato de imagen, marcas de corte, línea de marco, fuente, tamaño, estilo, color y posición del título, y el formato de exportación. No guarda la fuente de material ni el texto del título: pertenecen al proyecto. Los preajustes se quedan dentro de CineMosaic.

Abre el menú **Preajuste** abajo en la columna lateral:

- **Nuevo preajuste…**: escribe un nombre y haz clic en **Guardar** (Retorno también sirve, Esc cancela).
- Elegir un preajuste de la lista ajusta todos los campos a la vez.
- **Sustituir por los ajustes actuales**: el preajuste elegido toma los campos tal como están.
- **Renombrar…**: da un nombre nuevo al preajuste elegido.
- **Eliminar…**: pregunta una vez y luego elimina el preajuste elegido.

La fuente se guarda por su nombre: si este Mac no la tiene, la fuente se queda como está.

## Guardar

Elige el formato y haz clic en Guardar…. Si faltan fotogramas que la hoja necesita (tras cambiar el formato o la hoja), CineMosaic lo avisa antes: guardar de todos modos con celdas grises, o cancelar y capturarlos. PNG, TIFF y JPEG llevan la resolución, así que un programa de impresión los imprime al tamaño elegido. La página PDF tiene el tamaño del papel. Mostrar en el Finder muestra el archivo guardado.

## Ajustes y ayuda

CineMosaic › Ajustes… (⌘,) tiene Claro/Oscuro y el idioma (English, Deutsch, Français, Español). Actualizaciones: CineMosaic busca solo nuevas versiones en GitHub y muestra un aviso; Instalar actualización reemplaza la app y la reinicia. Soporte: una autoprueba con datos propios, Informar… para problemas e ideas (sin cuenta de GitHub) y el archivo de registro. Ayuda › Mostrar archivo de registro lo muestra en el Finder.

## Bueno saber

CineMosaic lee los fotogramas con FFmpeg. Los fotogramas se guardan en ~/Library/Caches/CineMosaic mientras la app está abierta y se borran al cerrarla (los restos de un fallo, en el siguiente inicio). Si macOS pregunta si CineMosaic puede acceder a archivos de un volumen extraíble o de red, permítelo: hasta que respondas, la lectura del volumen espera.

Avid y Media Composer son marcas de Avid Technology, Inc. CineMosaic no está afiliado a Avid.
