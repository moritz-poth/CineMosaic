# Un fotograma del centro de cada plano

## Qué hace CineMosaic

CineMosaic toma un fotograma del centro de cada plano de un montaje y los coloca todos en una hoja, plano tras plano, de izquierda a derecha y de arriba abajo. Ves la hoja mientras la configuras y la guardas como PNG, TIFF, JPEG o PDF.

Trabaja con un bin de Media Composer y las Avid MediaFiles de tus volúmenes, o con un máster del montaje y su lista de montaje. CineMosaic solo lee: nunca cambia tus medios, bins ni proyectos.

## Qué cuenta como plano

En cada momento cuenta la pista de vídeo más alta que tiene un clip: la imagen que ves. El relleno deja ver las pistas de abajo. Un clip cubierto en parte por un clip de una pista más alta da dos planos: antes y después del inserto. Los títulos y efectos en pistas superiores cuentan como imagen.

El fotograma es el del centro de cada plano. En un plano de 50 fotogramas es el fotograma 25. Si quedan celdas libres en la última fila, los planos más largos dan dos fotogramas en lugar de uno – tras el primer y tras el segundo tercio (con 90 fotogramas: los fotogramas 30 y 60) – hasta llenar la hoja. Si cambias la hoja después de capturar y necesita fotogramas que aún faltan, aparecen en gris y el botón dice «Capturar N fotogramas que faltan»: captura solo esos.

## Máster y lista de montaje

Elige Origen › Máster + lista de montaje. El máster es el montaje exportado (QuickTime, MXF y todo lo que lee ffmpeg). La lista de montaje puede ser una EDL (CMX3600), un XML de Final Cut Pro 7 (Premiere Pro, DaVinci Resolve), un FCPXML (Final Cut Pro, Resolve) o un AAF.

CineMosaic sitúa cada plano en el máster por su timecode: timecode del plano menos timecode inicial del máster. Si el máster empieza en 10:00:00:00, la lista de montaje también debe empezar ahí. Si hay planos antes del inicio del máster, CineMosaic lo indica bajo el origen.

Una EDL solo tiene una pista de vídeo y no tiene cadencia; CineMosaic toma la del máster.

## Bin de Media Composer

Elige Origen › Bin de Media Composer, después el bin (.avb). En Secuencias, cada secuencia del bin tiene su interruptor: Todas las secuencias (por defecto) las toma todas una tras otra, por orden de nombre (Reel 2 antes de Reel 10); activa solo las que quieras: una, o tres de cinco. Debajo aparecen todos los volúmenes con una carpeta «Avid MediaFiles»; desactiva los que CineMosaic no debe buscar. Los volúmenes que conectes después aparecen solos.

CineMosaic sigue cada plano a través de efectos, efectos de velocidad, multicámara y anidados hasta su archivo de medios. También encuentra clips de otros bins del proyecto. Los archivos vinculados con AMA (QuickTime y otros fuera de Avid MediaFiles) se leen donde Media Composer los vincula.

## Extraer los fotogramas

Haz clic en Extraer fotogramas. La barra muestra el avance y el tiempo restante; Pausa y Cancelar lo detienen en cualquier momento; Cancelar no guarda nada. Un fotograma que no se puede leer queda como un recuadro gris en la hoja; la línea bajo el origen nombra el primero y el motivo.

## Formato de imagen

Si tu imagen tiene barras negras, elige su formato en Hoja › Formato de imagen: formatos de cine (4:3, 1.66:1, 16:9, 1.85:1, 2:1, 2.35:1, 2.39:1) o de redes sociales (9:16 para Reels, TikTok, Shorts y Stories, 4:5, 3:4, 1:1, 2:3, 1.91:1, 16:9). Ancho : alto debajo muestra siempre el formato; escribe ahí uno propio (por ejemplo 2.39 : 1 o 21 : 9). CineMosaic recorta las barras alrededor del centro de la imagen: arriba y abajo en formatos anchos, a izquierda y derecha en estrechos. Fotograma completo conserva toda la imagen. Puedes cambiar el formato sin volver a extraer.

## La hoja

Ancho × alto siempre está a la vista: elige DIN A0 a A6 en la lista Papel para rellenarlo, o escribe tu propio tamaño; la lista muestra entonces Personalizado (o el formato A que coincide). Vertical u horizontal gira la hoja. Unidad cambia todas las longitudes – papel, margen, espacio, línea de marco y X e Y del título – entre mm y píxeles; el tamaño de letra sigue en puntos. Todas son números enteros. Resolución en dpi: 300 para imprimir, 150 para la pantalla. Margen es el borde vacío alrededor de todo, Espacio el hueco entre dos fotogramas: pulsa en el campo y arrastra a izquierda o derecha para cambiar el valor, o haz clic para escribirlo. Con Columnas automáticas, CineMosaic elige el número de columnas que hace los fotogramas más grandes; desactívalo para fijar el número tú. La línea de abajo muestra la cuadrícula y el tamaño de un fotograma en píxeles.

Las hojas muy grandes (A0 a 600 dpi) necesitan mucha memoria para guardarse; la línea bajo la hoja dice cuánta.

## Línea de marco, título y marcas de corte

La columna lateral tiene cuatro pestañas: Origen, Hoja, Línea de marco y Título. En Línea de marco, el interruptor dibuja una línea alrededor de la hoja: elige un patrón (simple, doble, grueso – fino, fino – grueso, fino – grueso – fino, dos juntas y una aparte), el grosor de una línea fina, la distancia al borde y el color (negro al principio; una muestra o un valor hexadecimal como #3c4146).

En Título escribe el título. La lista de fuentes muestra cada fuente de tu Mac con su propio aspecto; ↑ y ↓ la recorren y la hoja cambia al momento, una letra salta a la siguiente fuente que empieza así. Después tamaño en puntos, negrita, cursiva y color. El ancla es el punto de la hoja del que cuelga el título, uno de nueve, como en un programa de imagen; X e Y lo mueven desde ahí en mm o píxeles. También puedes arrastrar el título en la vista previa. Donde cubre la línea de marco, la línea se interrumpe a su alrededor. Arriba o abajo, los fotogramas guardan distancia; en el centro queda por encima de ellos.

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
