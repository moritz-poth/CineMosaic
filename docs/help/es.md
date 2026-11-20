# Un fotograma del centro de cada plano

## Qué hace CineMosaic

CineMosaic toma un fotograma del centro de cada plano de un montaje y los coloca todos en una hoja, plano tras plano, de izquierda a derecha y de arriba abajo. Ves la hoja mientras la configuras y la guardas como PNG, TIFF, JPEG o PDF.

Trabaja con un bin de Media Composer y las Avid MediaFiles de tus volúmenes, o con un máster del montaje y su lista de montaje. CineMosaic solo lee: nunca cambia tus medios, bins ni proyectos.

## Qué cuenta como plano

En cada momento cuenta la pista de vídeo más alta que tiene un clip: la imagen que ves. El relleno deja ver las pistas de abajo. Un clip cubierto en parte por un clip de una pista más alta da dos planos: antes y después del inserto. Los títulos y efectos en pistas superiores cuentan como imagen.

El fotograma es el del centro de cada plano. En un plano de 50 fotogramas es el fotograma 25.

## Máster y lista de montaje

Elige Origen › Máster + lista de montaje. El máster es el montaje exportado (QuickTime, MXF y todo lo que lee ffmpeg). La lista de montaje puede ser una EDL (CMX3600), un XML de Final Cut Pro 7 (Premiere Pro, DaVinci Resolve), un FCPXML (Final Cut Pro, Resolve) o un AAF.

CineMosaic sitúa cada plano en el máster por su timecode: timecode del plano menos timecode inicial del máster. Si el máster empieza en 10:00:00:00, la lista de montaje también debe empezar ahí. Si hay planos antes del inicio del máster, CineMosaic lo indica bajo el origen.

Una EDL solo tiene una pista de vídeo y no tiene cadencia; CineMosaic toma la del máster.

## Bin de Media Composer

Elige Origen › Bin de Media Composer, después el bin (.avb). En Secuencia, Todas las secuencias (por defecto) toma cada secuencia del bin una tras otra, por orden de nombre (Reel 2 antes de Reel 10); o elige una sola. Debajo aparecen todos los volúmenes con una carpeta «Avid MediaFiles»; desactiva los que CineMosaic no debe buscar. Los volúmenes que conectes después aparecen solos.

CineMosaic sigue cada plano a través de efectos, efectos de velocidad, multicámara y anidados hasta su archivo de medios. También encuentra clips de otros bins del proyecto. Los archivos vinculados con AMA (QuickTime y otros fuera de Avid MediaFiles) se leen donde Media Composer los vincula.

## Extraer los fotogramas

Haz clic en Extraer fotogramas. La barra muestra el avance y el tiempo restante; Pausa y Cancelar lo detienen en cualquier momento; Cancelar no guarda nada. Un fotograma que no se puede leer queda como un recuadro gris en la hoja; la línea bajo el origen nombra el primero y el motivo.

## Formato de imagen

Si tu imagen tiene barras negras, elige su formato en Hoja › Formato de imagen: 4:3, 1.66, 16:9, 1.85, 2.00, 2.35, 2.39 o uno propio (por ejemplo 2.39 o 21:9). CineMosaic recorta las barras alrededor del centro de la imagen: arriba y abajo en formatos anchos, a izquierda y derecha en estrechos. Fotograma completo conserva toda la imagen. Puedes cambiar el formato sin volver a extraer.

## La hoja

Papel: DIN A0 a A6 o Personalizado (ancho × alto en mm), vertical u horizontal. Resolución en dpi: 300 para imprimir, 150 para la pantalla. Margen es el borde vacío alrededor de todo, Espacio el hueco entre dos fotogramas, ambos en mm: pulsa en el campo y arrastra a izquierda o derecha para cambiar el valor, o haz clic para escribirlo. Con Columnas automáticas, CineMosaic elige el número de columnas que hace los fotogramas más grandes; desactívalo para fijar el número tú. La línea de abajo muestra la cuadrícula, el tamaño de la hoja en píxeles y el de un fotograma.

Las hojas muy grandes (A0 a 600 dpi) necesitan mucha memoria para guardarse; la línea bajo la hoja dice cuánta.

## Línea de marco, título y marcas de corte

La columna lateral tiene cuatro pestañas: Origen, Hoja, Línea y Título. En Línea, Línea de marco dibuja una línea alrededor de la hoja: elige un patrón (simple, doble, grueso – fino, fino – grueso, fino – grueso – fino, dos juntas y una aparte), el grosor de una línea fina, la distancia al borde y el color (negro al principio; una muestra o un valor hexadecimal como #3c4146).

En Título escribe el título. La lista de fuentes muestra cada fuente de tu Mac con su propio aspecto; ↑ y ↓ la recorren y la hoja cambia al momento, una letra salta a la siguiente fuente que empieza así. Después tamaño en puntos, negrita, cursiva y color. El ancla es el punto de la hoja del que cuelga el título, uno de nueve, como en un programa de imagen; X e Y lo mueven desde ahí en mm. También puedes arrastrar el título en la vista previa. Donde cubre la línea de marco, la línea se interrumpe a su alrededor. Arriba o abajo, los fotogramas guardan distancia; en el centro queda por encima de ellos.

Hoja › Marcas de corte compone la hoja en una zona con el formato de imagen dentro del papel –para fotogramas en 2:1, una zona en 2:1– y pone marcas de corte en sus esquinas para recortarla.

## Preajustes

Un preajuste guarda el aspecto de la hoja: papel, orientación, resolución, margen, separación, columnas, formato de imagen, marcas de corte, línea de marco, fuente, tamaño, estilo, color y posición del título, y el formato de exportación. No guarda la fuente de material ni el texto del título: pertenecen al proyecto.

En **Preajuste**, elige **Guardar como preajuste…** y dale un nombre. Los preajustes guardados aparecen en el mismo menú; elegir uno ajusta todos los campos a la vez. **Cargar desde archivo…** abre un preajuste desde cualquier lugar, por ejemplo uno que te haya enviado un colega. **Mostrar carpeta de preajustes** abre la carpeta en el Finder; allí se borra un preajuste. La fuente se guarda por su nombre: si un Mac no la tiene, la fuente se queda como está.

## Guardar

Elige el formato y haz clic en Guardar…. PNG, TIFF y JPEG llevan la resolución, así que un programa de impresión los imprime al tamaño elegido. La página PDF tiene el tamaño del papel. Mostrar en el Finder muestra el archivo guardado.

## Ajustes y ayuda

CineMosaic › Ajustes… (⌘,) tiene Claro/Oscuro y el idioma (English, Deutsch, Français, Español). Actualizaciones: CineMosaic busca solo nuevas versiones en GitHub y muestra un aviso; Instalar actualización reemplaza la app y la reinicia. Soporte: una autoprueba con datos propios, Informar… para problemas e ideas (sin cuenta de GitHub) y el archivo de registro. Ayuda › Mostrar archivo de registro lo muestra en el Finder.

## Bueno saber

CineMosaic lee los fotogramas con FFmpeg. Los fotogramas se guardan en ~/Library/Caches/CineMosaic mientras la app está abierta y se borran al cerrarla (los restos de un fallo, en el siguiente inicio). Si macOS pregunta si CineMosaic puede acceder a archivos de un volumen extraíble o de red, permítelo: hasta que respondas, la lectura del volumen espera.

Avid y Media Composer son marcas de Avid Technology, Inc. CineMosaic no está afiliado a Avid.
