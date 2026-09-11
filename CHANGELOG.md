# Registro de cambios

Todo lo que ha ido cambiando en Vitela, contado en lo que se nota al usarla
y no en lo que se ha tocado por dentro. Lo más reciente, arriba.

**Sobre los números de versión.** Vitela se ha ido construyendo por ciclos
de paridad con Acrobat, y hasta el ciclo 7 no hubo una versión publicada por
ciclo: `v0.1.0` y `v0.2.0` son las dos únicas que llegaron a salir y
recogen todo lo anterior. Desde el ciclo 8 el número acompaña al ciclo —el
0.3.0 y el 0.4.0 se quedaron sin publicar: el 0.4.0 cierra el ciclo 9 en el
código, pero la primera versión que sale después de la 0.2.0 es la 0.5.0—.
Cada apartado de aquí abajo es un ciclo, tenga versión propia o no.

---

## 0.5.0 — ciclo 10 (cierre)

El ciclo que lleva la tabla de paridad a su techo: no abre funciones
nuevas, arregla lo que estaba a medias y se somete a la regresión completa.

### Añadido

- Un PDF cifrado para unos destinatarios se abre con tu certificado y su
  clave privada (.p12 o PEM), con el mismo selector que firmar.
- Al firmar con LTV se consulta si el certificado seguía vigente (OCSP) y la
  prueba viaja dentro del PDF; el panel de firmas dice que está archivada y
  de cuándo es, leyéndola del documento y sin salir a la red.
- Comparar mira también las imágenes, no solo el texto, y ocupa la ventana
  entera con las hojas al ancho de su panel, recorrible con el teclado.
- Buscar en una carpeta tiene su entrada en el menú Editar (⇧⌘F), funciona
  sin ningún documento abierto, tiene botón, recuerda la carpeta y si mira
  las subcarpetas, y al cancelar dice hasta dónde llegó.
- Exportar imágenes acepta un rango de páginas; recortar acepta márgenes
  numéricos; al corregir un texto se avisa antes si va a cambiar de letra.
- Los destinatarios del cifrado se enseñan por su nombre, no por el del
  fichero del certificado.
- Este registro de cambios.

### Cambiado

- Los comentarios y los campos rellenados salen también en el folleto, en
  varias páginas por hoja y en el póster; «Solo el documento» los quita.
- Recuperar un documento tras un cierre inesperado ya no lo copia otra vez.
- La biblioteca de imágenes distingue de verdad firma, iniciales y sello, se
  puede cambiar la ranura y borrar pide confirmación.
- El fondo de color sólido puede ser opaco, y lo es por defecto.
- Guardar respeta con qué programa se hizo el original.
- «Abrir…» y «Guardar» están en el grupo Archivo del menú Acciones, y
  «Quitar la contraseña…» se atenúa con su motivo en vez de esconderse.
- Los mensajes dicen el nombre del fichero y dejan la ruta completa para el
  título; el sello dinámico pone tu nombre aunque no lo hayas escrito en
  preferencias.

### Corregido

- Subir o bajar una página borraba los marcadores, la numeración, los
  adjuntos, la vista inicial y el formulario entero.
- El póster se componía en miles de hojas: el porcentaje se leía como
  factor.
- Imprimir en folleto, N-up o póster no imprimía nada; «Vista inicial» no
  se guardaba nunca; la banda decía «con sello de tiempo» aunque el servidor
  no hubiera contestado.
- Un PDF firmado y luego modificado se decía «no comprobado» en vez de
  «modificado».
- Los avisos de progreso y de error quedaban escondidos detrás del velo de
  un diálogo abierto.
- El Optimizer no contaba la imagen del fondo y enseñaba sus categorías en
  la jerga del motor.
- Esc no cerraba el desplegable de reglas, guías y cuadrícula.

---

## 0.4.0 — ciclo 9

### Añadido

- **Buscar en todos los PDF de una carpeta**, desde el mismo campo de
  búsqueda de siempre: con el progreso a la vista, con Cancelar —que
  conserva lo que ya se había encontrado— y diciendo cuántos ficheros no se
  han podido abrir y por qué.
- **Certificar el documento** con el nivel a la vista: qué se podrá cambiar
  después sin romper el sello, escrito en tres frases y no con los números
  del formato.
- **Sello de tiempo de una autoridad** al firmar, con la hora sellada y
  quién responde por ella; si el servidor no contesta, se pregunta en vez de
  tirar la firma.
- **Imprimir en folleto, varias páginas por hoja o póster**, con la hoja
  dibujada y el orden real de las páginas.
- **El Optimizer dice de qué está hecho el fichero** antes de tocar nada, y
  cada casilla lleva escrito lo que ahorra.
- **Exportar a página web (.html)**, con las imágenes al lado y los enlaces
  que se pueden seguir.
- **Comparar dos documentos** lado a lado, con las diferencias señaladas y
  sin tocar ninguno de los dos ficheros.
- **Cifrar para unos destinatarios** en vez de con una contraseña.
- **Galería de sellos** con vista previa, incluidos los sellos dinámicos que
  llevan quién sella y cuándo.
- **Fondo de color sólido** y su forma de quitarlo.
- El **fichero de una chincheta** se abre con doble clic y se guarda donde
  se quiera.
- **La vista inicial** del documento en las propiedades: con qué página,
  qué zoom y qué disposición se abre en cualquier visor.
- **Reglas, guías y cuadrícula** en el menú Ver, con su estado y su atajo.

### Cambiado

- El zoom llega hasta el 6400 % y baja hasta el 8 %.
- «Ir a la página» entiende la etiqueta que enseña la píldora («xii»), no
  solo el número.
- Al insertar un PDF se elige qué páginas entran.
- Las formas se guardan con su tipo de verdad, así que fuera de Vitela son
  un rectángulo, un círculo o una flecha, no un dibujo a mano.

### Corregido

- La ventana ya no se queda en blanco si algo falla al pintar: el fallo se
  queda en su trozo y hay una salida.
- Recuperar un documento ya no lo duplica en el disco.
- Cuando la aplicación manda algo que el motor no espera, se explica en
  llano en vez de enseñar el nombre de un módulo.

---

## 0.3.0 — ciclo 8 (no llegó a publicarse)

### Añadido

- **Etiquetas de página**: el documento se puede numerar como dicen sus
  portadillas (i, ii, iii… y luego 1, 2, 3), y la píldora y las miniaturas
  lo dicen.
- **Fondo**: la marca de agua debajo del contenido, no solo encima.
- **Numeración Bates** con su diálogo entero, prefijo y sufijo incluidos.
- **Propiedades del documento** (⌘D) con la ficha del fichero: peso,
  versión, tamaño de página, formulario, seguridad y las fuentes con su
  tipo y si vienen dentro.
- **Adjuntar un fichero a un punto de la página**, como comentario, con su
  chincheta.
- **Los datos de un formulario salen y vuelven** en un fichero aparte
  (XFDF), para mandarlos y recibirlos rellenos.
- **La llamada puede llevar codo**, con sus dos tramos.
- **Reglas, guías y cuadrícula** para colocar sin ojímetro, guardadas por
  documento y sin tocar el fichero.
- **Iniciales guardadas**, la segunda ranura de la biblioteca de firmas.
- **Imprimir el resumen de comentarios** detrás del documento.
- **Guardar una imagen del documento** como PNG desde su propio popover.

### Cambiado

- El zoom salta por los niveles de Acrobat en vez de ir de dos en dos.
- Recuperar un cierre inesperado ofrece **todos** los documentos que
  estaban abiertos, uno por pestaña, y dice cuándo fue cada uno.
- ⌘W cierra la pestaña de delante, con su pregunta si tiene cambios.
- La medida que se deja puesta es un comentario de verdad y ya no ensucia
  el texto del documento.

### Corregido

- Seguir un marcador ya llega a su sitio: antes el salto y el ajuste de la
  altura competían y el usuario se quedaba donde estaba.
- El punto de «documento sin guardar» se apaga cuando ⌘Z devuelve el
  documento a como se abrió.

---

## Ciclo 7

### Añadido

- **Varios documentos a la vez**, en pestañas, que solo se pintan a partir
  del segundo.
- **Reconocer campos de formulario**: se proponen y no se escribe nada
  hasta decirlo, se pueden renombrar y quitar uno a uno, y crearlos todos es
  un solo paso de deshacer.
- **Marcadores con ⌘B** y con destino fino: volver a uno devuelve la vista
  donde se puso, no el principio de la página.
- **Los comentarios salen en tres formatos** —texto, resumen en PDF y
  XFDF— y **vuelven a entrar**.
- **Medir**, con las tres herramientas de Acrobat: distancia, perímetro y
  área, con la escala fijada por documento.
- **Modo lectura** (⇧⌘H) y la **herramienta Mano** (barra espaciadora
  mantenida).

### Cambiado

- El espaciado entre caracteres vuelve a estar en la fila del modo Editar y
  ahora sobrevive a mover y a estirar el bloque.
- La goma es un solo paso de deshacer por pasada, no uno por trazo.
- Un campo de solo lectura se pinta apagado y lo dice; el título que trae el
  PDF sale al pasar el ratón.
- Con varias firmas y ninguna tocada, la banda dice que el documento no ha
  cambiado desde la última: una revisión detrás no es una manipulación.

### Corregido

- Un grupo de botones de radio se podía marcar por fin: el estado se lee de
  donde lo escribe el visor y no de donde el motor se lo inventaba.
- Corregir una línea ya no se lleva por delante el resto del párrafo.
- «Crear PDF desde imágenes» descuenta las que no se han podido leer y las
  marca en su fila en vez de tirar el lote.

---

## Ciclo 6

### Añadido

- **Llamada**: un cuadro de texto con su línea y su flecha señalando al
  documento.
- **Goma de borrar** dentro del modo Dibujar.
- **Formularios completos**: botones de radio con su grupo, desplegables,
  listas y el bloque de propiedades de Acrobat (ayuda, obligatorio, solo
  lectura, valor por defecto y orden de tabulación).
- **Crear un PDF desde imágenes**, en A4, carta o al tamaño de cada foto.
- **Adjuntos**: se pueden quitar del documento y abrir con el visor del
  sistema.
- **Resumen de comentarios** en un fichero de texto, con el nombre del
  documento y las fechas en español.

### Cambiado

- **El párrafo refluye al corregir texto**: se reescribe la línea y las de
  abajo se recolocan solas, sin cruzar bloques ni páginas, como Acrobat.
- Firmar un documento ya firmado **no toca la firma anterior**: el fichero
  crece por el final.
- Las firmas se nombran en llano —el nombre de la persona y el de quien
  emitió su certificado— en vez del identificador completo.
- Apagar una capa avisa de que eso cambia el fichero.

### Corregido

- Un catálogo con las capas escritas de otra forma ya no responde «desde
  aquí no se pueden apagar».
- Los cuadros de texto parten las líneas al ancho de su caja, así que el
  texto ya no se sale por el borde derecho.

---

## Ciclo 5

### Añadido

- **Comentarios con conversación**: responder a uno, y los cuatro estados de
  revisión de Acrobat, guardados como los guarda Acrobat.
- **Adjuntos y capas** del documento, con su pestaña en el panel lateral.
- **Exportar a Word (.docx)**, con el aviso de lo que no va a salir dicho
  antes de elegir dónde guardarlo.
- **Mover y estirar un bloque de texto** con los ocho tiradores, y
  **recortar una imagen** de verdad, quitando lo que sobra del fichero.
- **Cadena de confianza** en las firmas: quién responde por el certificado,
  con el almacén del sistema, y sin decir nunca «válida» porque no se
  comprueba la revocación.

### Cambiado

- El color, la negrita y la cursiva de un bloque de texto se leen del
  documento, y el tamaño que se enseña es el que se ve.
- El interlineado se puede elegir al escribir o al corregir.
- Un campo de firma dejó de contarse como campo de formulario: un PDF que
  solo lleva una firma ya no se anuncia como rellenable.

### Corregido

- **Importar una página con una nota ya no cierra la aplicación**: era el
  fallo más grave que ha tenido el proyecto y se arregló en los nueve sitios
  por los que se importan páginas, no solo donde apareció.
- Extraer páginas comprueba antes de escribir nada que se puede, en vez de
  dejar el trabajo a medias.
- La sesión sin guardar se apunta de verdad: antes la llamada se rechazaba
  y no había nada que recuperar.

---

## Ciclo 4

### Añadido

- **Reemplazar todo**, en una sola operación que un ⌘Z deshace entera, con
  la lista de coincidencias y su frase de alrededor.
- **Marcas de redacción en dos fases**: se marcan, se ven como lo que son
  en cualquier visor, se mueven y se borran, y aplicarlas es un paso aparte.
- **Autoguardado y recuperación** tras un cierre inesperado.
- La marca de agua puede ser **una imagen**, con su opacidad, su giro y su
  rango de páginas.
- Las imágenes se **giran y se voltean**, y se mandan al frente o al fondo.
- El texto nuevo se puede **alinear y colorear**.

### Cambiado

- **La verificación de firmas es honesta**: tres estados en vez de dos, y lo
  que no se sabe leer sale como «no se ha podido comprobar» y nunca en rojo.
  Se entienden además las firmas ECDSA y las que usan SHA-384 o SHA-512.
- Los recientes traen su ficha —páginas, peso y el candado si van cifrados—
  sin abrir el documento.

### Corregido

- El menú del sistema dejó de ser decorativo: sus entradas hacen lo mismo
  que su botón, y las que no aplican se atenúan en vez de no responder.
- Imprimir ya no saca un folio en blanco por página.
- Quitar una marca de redacción ya no borra el comentario de al lado.

---

## Ciclo 3

### Añadido

- **Verificación de firmas al abrir**, con la banda que dice en llano quién
  firmó y si el documento ha cambiado desde entonces.
- **Firma visible**: se dibuja el recuadro en la página y ahí sale la firma
  manuscrita, el nombre, la fecha y el motivo.
- **Quitar información oculta** del documento: metadatos, scripts,
  adjuntos, capas y formularios.
- **Reemplazar páginas**, **dividir el documento** y **un fichero por
  página**.
- **Menú del sistema** completo: Archivo, Editar, Ver, Documento, Ventana y
  Ayuda, espejo del menú de la aplicación.
- **Cuatro modos de presentación**: una página, continua, dos páginas y dos
  continuas, con portada.

### Cambiado

- Buscar puede traer la frase de alrededor de cada coincidencia.
- El diálogo de imprimir cuenta cuántas hojas van a salir y avisa cuando el
  rango no deja ninguna.

---

## Ciclo 2

### Añadido

- **Panel de comentarios** con todo el documento en una lista, con filtros
  por tipo y por autor.
- **Corregir y recolorear** un comentario ya puesto, al instante.
- **Cuadro de texto** encima del documento.
- **Borrar y girar páginas en lote**, cada lote en un solo paso de
  deshacer.
- **Desplegables y listas** en los formularios.
- **Proteger con contraseña** (AES-256) con sus permisos, y **quitar la
  protección**.
- **Fijar las anotaciones en la página**, **redactar**, **exportar como
  imágenes**, **exportar el texto** y **reducir el tamaño**.
- **Deshacer y rehacer** cualquier cambio del documento, hasta veinte pasos.
- **Ficheros recientes**, con su ficha en el estado vacío.

### Cambiado

- Lo que se resalta, se subraya o se tacha **existe fuera de Vitela**: se
  ve, se imprime y sobrevive a aplanar en cualquier visor.
- Los comentarios llevan su autor y su fecha con la zona horaria, así que
  dos comentarios de husos distintos se ordenan bien.

### Corregido

- Cerrar la ventana con cambios sin guardar pregunta antes en vez de tirar
  el trabajo.
- Las páginas giradas dejaron de desplazar lo que se pone encima.

---

## 0.2.0 y anteriores — el visor y el editor

Lo que Vitela ya era antes del programa de paridad: abrir y ver un PDF al
instante, seleccionar y buscar texto, unir, separar, reordenar, girar y
borrar páginas, resaltar, dibujar y poner notas, rellenar formularios,
**editar el texto de verdad** —reescribiendo el documento, no pegando un
parche encima—, insertar y colocar imágenes, y firmar con certificado.
