# Glassbar: especificación de diseño

Fecha: 2026-10-30
Estado: aprobado en brainstorming, pendiente de revisión final del usuario
Sustituye a: BatLauncher (WPF, .NET 9), que pasa a la rama `legacy-wpf`

## 1. Qué es

Glassbar es una segunda barra de herramientas invisible para Windows. Se abre al pulsar su icono anclado en la barra de tareas, aparece sobre ella como un widget de cristal con los iconos de tus scripts y programas, lanzas uno con un clic y desaparece. Está escrita en Rust sobre Win32, DirectComposition y Direct2D, se distribuye como un único exe sin dependencias y no deja ningún proceso corriendo salvo que tú lo pidas.

## 2. Objetivos y prioridades

En orden:

1. **Rendimiento percibido.** Primer frame en menos de 50 ms en frío. La apertura se siente instantánea y suave desde la primera vez.
2. **Cero coste para el sistema.** Sin proceso residente por defecto. Si se activa el modo residente, menos de 15 MB de memoria privada y ninguna actividad de CPU en reposo: sin temporizadores ni sondeos.
3. **Estabilidad frente a actualizaciones de Windows.** Solo API Win32 pública y estable. Ningún runtime externo. Ninguna API no documentada.
4. **Sensación de widget de Windows 11.** Cristal, esquinas redondeadas, animaciones del compositor, tema claro u oscuro según el sistema.
5. **Mantenible.** Módulos pequeños con una responsabilidad, tests unitarios para toda la lógica que no sea dibujo.

## 3. Alcance

### Incluido en la versión 1.0.0

- Barra con iconos de entradas `.bat`, `.cmd`, `.exe` y `.lnk`.
- Apertura desde el icono anclado; en modo residente también desde atajo global e icono de bandeja.
- Menú contextual de cristal por icono, botón de añadir, arrastrar y soltar archivos sobre la barra.
- Diálogo de nueva entrada y editar, ventana de ajustes, diálogo de confirmación, todos de cristal.
- Carrusel horizontal cuando las entradas no caben en el 80 % del monitor.
- Importación automática de los datos de BatLauncher en el primer arranque.
- Idiomas español e inglés.
- Comprobación de actualizaciones contra GitHub Releases y actualización con un clic.
- Workflow de GitHub Actions que publica las releases.

### Fuera de alcance en 1.0.0

- Reordenar arrastrando iconos dentro de la barra. Se reordena desde el menú contextual. El layout lo permite para una versión posterior.
- Opciones por entrada: argumentos, ejecutar como administrador, ocultar consola.
- Modo de nombres bajo los iconos. Solo tooltip.
- Más idiomas. La infraestructura queda lista: añadir un idioma es añadir un archivo.
- Firma de código. En la primera ejecución SmartScreen puede avisar.
- Lectores de pantalla. Hay navegación por teclado, pero no UI Automation.
- Compilación ARM64. El código no lo impide; se añade al workflow cuando haga falta.
- Windows 10 anterior a 1903.

## 4. Decisiones tomadas

| Tema | Decisión |
|---|---|
| Motor | Rust + Win32 + DirectComposition + Direct2D + DirectWrite. Crates: `windows`, `serde`, `serde_json`. Sin runtime asíncrono ni frameworks de UI. |
| Windows mínimo | Windows 10 1903 x64. Mica en ajustes solo en Windows 11 22H2 o superior; fondo sólido en el resto. |
| Proceso | Bajo demanda por defecto. Ajuste "mantener en segundo plano" para modo residente con icono de bandeja. "Iniciar con Windows" y atajo global solo en residente. |
| Abrir | Clic en el icono anclado siempre. Atajo global y bandeja en residente. |
| Ocultar | Al lanzar una entrada, con Escape, al perder el foco, y al volver a pulsar el icono anclado, el atajo o la bandeja. No hay botón de cerrar en la barra. |
| Posición | Centrada sobre la barra de tareas del monitor donde está el cursor. Sin arrastre. |
| Entradas | `.bat`, `.cmd`, `.exe`, `.lnk`. Sin opciones por entrada. `.bat` y `.cmd` con consola visible. |
| Gestión | Sin ventana de gestión. Menú contextual, botón + y arrastrar archivos. |
| Reordenar | Mover a la izquierda o a la derecha desde el menú contextual. |
| Desbordamiento | Carrusel horizontal con arrastre, inercia, rueda, bordes desvanecidos y flechas. Añadir y ajustes fijos a la derecha. |
| Cristal de la barra | Acrylic por instantánea: captura de lo que hay detrás, desenfoque con Direct2D una vez por apertura. Permite animar el plato. Igual en Windows 10 y 11. |
| Dispositivo de dibujo | Direct3D 11 WARP, siempre. Crear el dispositivo por hardware carga el driver de la GPU y tarda entre 350 ms y 2,6 s por proceso en la máquina de referencia; WARP tarda 28 ms. La composición y las animaciones las ejecuta DWM en la GPU en ambos casos. Medido en `docs/perf.md`. |
| Cristal de ventanas secundarias | Mica del sistema cuando exista; sólido si no. |
| Tema | Según el sistema, con ajuste para forzar claro u oscuro. Color de acento de Windows. |
| Celda | 48 px con icono de 34 px. |
| Tooltip | A los 120 ms. Sin modo de etiquetas. |
| Animación | Ritmo suave: plato 260 ms, icono 220 ms, cascada 38 ms. Cierre 110 ms. |
| Menú contextual | De cristal, propio. |
| Ajustes y diálogos | De cristal, propios, estilo tarjetas de Ajustes de Windows 11. Cambios aplicados al instante. |
| Importación | Automática en el primer arranque. Sin botón en ajustes. |
| Idiomas | Español e inglés. Según el sistema por defecto. |
| Actualizaciones | GitHub Releases. Comprobación automática diaria o manual. Reemplazo del exe solo al pulsar Actualizar. |
| Nombre | Glassbar. Exe `Glassbar.exe`, carpeta `%LOCALAPPDATA%\Glassbar`, repo `TermiSenpai/Glassbar`. |
| Repositorio | El mismo. El código WPF actual, incluidos los cambios sin confirmar, se guarda en la rama `legacy-wpf`. El proyecto Rust ocupa la raíz de `main`. |

Supuesto a confirmar: el repositorio de GitHub `BatLauncher` se renombra a `Glassbar`. GitHub redirige el nombre antiguo. Si no se renombra, la constante del actualizador apunta a `TermiSenpai/BatLauncher`.

## 5. Experiencia de usuario

Todas las medidas son píxeles lógicos. Se multiplican por la escala del monitor.

### 5.1 La barra

- Plato: radio 12, borde de 1 px, sombra suave. Relleno interior 6 vertical y 8 horizontal.
- Celdas de 48 × 48 con radio 9, separación 4. Icono de 34 × 34 centrado, con radio 7 si es una imagen.
- Tras las entradas: separador de 1 × 26 con margen 4, botón añadir, botón ajustes. Estos tres elementos son fijos y nunca se desplazan.
- Altura total 60. Ancho igual al contenido, hasta el límite del carrusel.
- Sin entradas: solo añadir y ajustes, sin separador, y un texto corto dentro del plato: "Pulsa + o arrastra un archivo".

Estados de una celda:

| Estado | Aspecto |
|---|---|
| Normal | Icono sobre el plato. |
| Hover | Fondo blanco al 10 % en oscuro o negro al 6 % en claro, escala 1.06 y 1 px hacia arriba, 120 ms. |
| Pulsado | Escala 0.94, 80 ms. |
| Foco por teclado | Anillo de 2 px en color de acento. |
| Archivo no encontrado | Icono al 35 % y en gris, punto rojo de 9 px abajo a la derecha. El clic no lanza nada. |
| Sin icono | Glifo de terminal en color de acento. Es lo que muestran `.bat` y `.cmd` sin icono propio. |

Botón de ajustes: muestra un punto de 7 px en color de acento cuando hay una versión nueva conocida.

### 5.2 Tooltip

- Aparece 120 ms después de entrar en una celda, 10 px por encima, centrado y dentro del monitor. Fundido de 120 ms.
- Texto: el nombre. Si el archivo no existe, segunda línea "Archivo no encontrado".
- Desaparece al salir de la celda, al pulsar o al empezar un arrastre. Pasar a la celda vecina lo mueve sin esperar.
- Es una ventana popup propia sin activación, con el mismo cristal que la barra.

### 5.3 Carrusel

- Ancho máximo del área de iconos: 80 % del ancho del área de trabajo del monitor menos el ancho de los controles fijos.
- Si las entradas caben, no hay carrusel y la barra mide lo que su contenido.
- Si no caben: el área de iconos se recorta a ese ancho, los bordes se desvanecen en 22 px por el lado en el que queda contenido oculto, y aparecen dos flechas de 20 px de ancho en los extremos del área. Cada flecha se atenúa al llegar al tope de su lado.
- Rueda: una celda por paso, 52 px, animado en 150 ms.
- Flecha: avanza el ancho visible menos una celda, animado en 220 ms.
- Arrastre: empieza tras mover 4 px. Durante el arrastre no hay hover ni tooltip y al soltar no cuenta como clic. Al soltar, inercia con desaceleración suave de hasta 400 ms, sin pasarse de los topes.
- Teclado: flechas izquierda y derecha mueven el foco y desplazan el carrusel para que la celda enfocada sea visible.

### 5.4 Apertura, cierre y posición

- Monitor: el que tiene el cursor en el momento de abrir.
- Posición: centrada horizontalmente en el área de trabajo. Si la barra de tareas está arriba, 8 px por debajo del borde superior del área de trabajo. En cualquier otro caso, 8 px por encima del borde inferior.
- Apertura, ritmo suave:
  - Plato y controles fijos: opacidad 0 a 1, desplazamiento vertical 18 a 0, escala 0.98 a 1, 260 ms, curva `cubic-bezier(0.16, 0.9, 0.3, 1)`.
  - Icono número i, de izquierda a derecha: opacidad 0 a 1, desplazamiento vertical 8 a 0, 220 ms, retraso 30 + 38·i ms.
  - Solo entran en cascada los iconos visibles del carrusel.
- Cierre: opacidad 1 a 0, desplazamiento vertical 0 a 10, escala 1 a 0.985, 110 ms, curva `cubic-bezier(0.4, 0, 1, 1)`. Sin cascada.
- Con "reducir movimiento" activo en Windows, o con el ajuste de animaciones apagado, todo es instantáneo: sin transiciones de hover ni de carrusel.
- Las transiciones propias de DWM al mostrar la ventana se desactivan para no duplicar la animación.

Toma el foco al abrirse para poder detectar Escape y la pérdida de foco. Se oculta cuando el foco pasa a una ventana que no sea suya. Sus ventanas propias son el tooltip, el menú, los diálogos y los selectores de archivos nativos abiertos desde ellos.

### 5.5 Lanzar una entrada

- Clic izquierdo o Enter sobre una celda: se lanza el archivo y la barra se oculta en cuanto el proceso arranca.
- `.bat` y `.cmd`: verbo `open` del shell, consola visible, igual que un doble clic en el Explorador.
- `.exe`: verbo `open`.
- `.lnk`: el shell resuelve el acceso directo, sus argumentos y su directorio.
- Directorio de trabajo: la carpeta del archivo.
- Si el archivo no existe, no se lanza nada; la celda ya lo indica.
- Si el lanzamiento falla, la barra no se oculta y la celda muestra durante 2,5 s un tooltip con el error del sistema.

### 5.6 Menú contextual

Clic derecho, tecla Menú o Mayús+F10 sobre una celda:

1. Abrir
2. Abrir ubicación del archivo. Abre el Explorador con el archivo seleccionado.
3. Separador
4. Editar…
5. Mover a la izquierda. Deshabilitado en la primera.
6. Mover a la derecha. Deshabilitado en la última.
7. Separador
8. Quitar de la barra. Pide confirmación.

Clic derecho en zona vacía del plato:

1. Nueva entrada…
2. Ajustes…

El menú es una ventana popup de cristal con iconos a la izquierda, elemento resaltado con fondo blanco al 10 %, navegación con flechas, Enter y Escape, y se cierra al perder el foco. Al cerrarse, la barra recupera el foco y sigue visible.

### 5.7 Añadir entradas

- Botón + o "Nueva entrada…": abre el diálogo de edición vacío. La barra se oculta y el diálogo queda solo.
- Arrastrar uno o varios archivos sobre la barra: se añaden al final sin diálogo. Las extensiones no admitidas se ignoran y la barra muestra 2,5 s el aviso "Tipo de archivo no admitido".
- Reglas al elegir un archivo, tanto en el diálogo como al soltar:
  - Nombre: el del archivo sin extensión, si el usuario no había escrito uno.
  - Un `.lnk` se resuelve a su archivo original: la entrada guarda la ruta del destino, sus argumentos y su directorio de trabajo, y usa el nombre del acceso directo. Así el acceso directo puede borrarse. Si el destino no es un archivo, se guarda el `.lnk` tal cual.
  - Icono: `.exe` y `.lnk` usan el icono del archivo; `.bat` y `.cmd` quedan sin icono.
- El diálogo de nueva entrada y editar también acepta archivos arrastrados: el primero admitido rellena la ruta como si se hubiera elegido con Examinar.

### 5.8 Diálogo de nueva entrada y editar

Ventana de 440 px de ancho, título "Nueva entrada" o "Editar entrada", botón de cerrar arriba a la derecha.

- Vista previa del icono de 64 px y botones: "Cambiar icono…" abre el selector nativo con filtro de imágenes PNG, JPG, ICO, BMP y ejecutables EXE y DLL de los que extraer el icono. "Usar el del archivo" solo está activo con `.exe` y `.lnk`. "Quitar" deja la entrada sin icono.
- Nombre: campo de texto obligatorio, máximo 64 caracteres.
- Archivo: campo de texto y botón "Examinar…" con filtro `.bat;.cmd;.exe;.lnk` y "Todos los archivos". Si la ruta no existe se avisa en rojo debajo, pero se permite guardar.
- Guardar y Cancelar. Enter guarda, Escape cancela. Guardar deshabilitado si el nombre o el archivo están vacíos.
- Las imágenes elegidas se copian a `icons\<id>.<ext>`. Los iconos extraídos de un exe se guardan como `icons\<id>.png`. Al cambiar o quitar, el archivo anterior se borra.

Confirmación de quitar: popover de cristal anclado sobre la celda, con el mismo plato y animación que el menú, el texto con el nombre de la entrada y los botones "Cancelar" y "Quitar". Enter confirma, Escape o clic fuera cancela. La barra sigue visible. Quitar borra su icono copiado.

### 5.9 Ventana de ajustes

Ventana de 520 px de ancho con tarjetas, sin barra de título del sistema, con cabecera propia "Ajustes" y botón de cerrar. Se arrastra desde la cabecera. Aparece centrada en el monitor donde estaba la barra, en la barra de tareas y en Alt+Tab. Cada cambio se aplica y se guarda al momento.

General

| Ajuste | Control | Por defecto | Notas |
|---|---|---|---|
| Idioma | Desplegable: Según el sistema, Español, English | Según el sistema | Cambia los textos al instante. |
| Mantener en segundo plano | Interruptor | Apagado | Al cerrar la barra el proceso queda en la bandeja. Al apagarlo se quitan la bandeja, el atajo y el inicio con Windows. |
| Iniciar con Windows | Interruptor | Apagado | Requiere el anterior; atenuado si está apagado. Escribe o borra la clave de registro. |
| Atajo de teclado global | Campo de captura | Ninguno | Requiere segundo plano. Clic y pulsar la combinación; Retroceso la borra; Escape cancela. Si Windows rechaza el atajo: "Ese atajo lo usa otra aplicación". |

Apariencia

| Ajuste | Control | Por defecto |
|---|---|---|
| Tema | Desplegable: Según el sistema, Claro, Oscuro | Según el sistema |
| Animaciones | Interruptor | Encendido. Sin efecto si Windows tiene "reducir movimiento". |

Actualizaciones

| Elemento | Comportamiento |
|---|---|
| Buscar actualizaciones automáticamente | Interruptor, encendido por defecto. |
| Fila de versión | "Versión X.Y.Z" y debajo el estado: "Última comprobación hace N h", "Comprobando…", "Nueva versión X.Y.Z disponible · ver cambios", "Estás al día", "Sin conexión" o el error. "Ver cambios" abre la página de la release. |
| Buscar ahora | Comprueba en el momento. |
| Actualizar | Solo visible con versión nueva. Descarga con porcentaje, verifica y reinicia la app. |

Datos

| Elemento | Comportamiento |
|---|---|
| Carpeta de datos | Muestra la ruta y el botón "Abrir carpeta". |

Pie: "Glassbar X.Y.Z · MIT · github.com/TermiSenpai/Glassbar" con el enlace clicable.

### 5.10 Modo residente

- Icono en la bandeja con el icono de la app y tooltip "Glassbar".
- Clic izquierdo: muestra u oculta la barra.
- Menú: Abrir, Ajustes…, separador, Salir.
- Atajo global: muestra u oculta la barra.
- "Iniciar con Windows" arranca el proceso oculto en la bandeja con `--background`.

### 5.11 Teclado

En la barra: flechas mueven el foco entre celdas y controles fijos, Enter activa, Menú o Mayús+F10 abre el contexto, Escape oculta. Al abrirse no hay celda enfocada; la primera flecha enfoca la primera celda visible. En diálogos: Tab recorre los controles, Espacio activa interruptores y botones, Enter confirma y Escape cierra.

## 6. Arquitectura

Un único crate binario. Un hilo de interfaz que ejecuta el bucle de mensajes de Win32. Hilos de trabajo solo cuando hacen falta: extracción de iconos y red.

### 6.1 Módulos

| Módulo | Responsabilidad | Depende de |
|---|---|---|
| `main` | Argumentos, instancia única, arranque según modo. | `app` |
| `app::instance` | Mutex con nombre `Local\Glassbar.SingleInstance` y ventana de mensajes de clase `Glassbar.Messages`. Una segunda instancia concede a la primera permiso de primer plano con `AllowSetForegroundWindow`, envía el mensaje registrado `Glassbar.Toggle` y sale. | Win32 |
| `app::lifecycle` | Decide residente o salir al ocultar. Bandeja, atajo global y clave de inicio con Windows. | `model::settings`, `ui::window` |
| `ui::window` | Crea ventanas popup sin bitmap de redirección, DPI por monitor, posición, mostrar y ocultar con animación, pérdida de foco, Escape, arrastrar archivos. | `gfx` |
| `ui::toolbar` | Layout de celdas y controles fijos, carrusel, hit-testing, hover, pulsado, foco, tooltip, lanzamiento. | `ui::window`, `gfx`, `model`, `icons`, `launch` |
| `ui::menu` | Menú contextual de cristal. | `ui::window`, `ui::widgets` |
| `ui::widgets` | Interruptor, desplegable, botón, enlace, campo de texto básico, captura de atajo, tarjeta, disposición vertical. | `gfx`, `i18n` |
| `ui::settings` | Ventana de ajustes. | `ui::widgets`, `model::settings`, `update`, `app::lifecycle` |
| `ui::edit_entry` | Diálogo de nueva entrada y editar, y confirmación de quitar. | `ui::widgets`, `model::entries`, `icons` |
| `gfx::compositor` | Dispositivo DirectComposition, árbol de visuales, superficies, animaciones con curvas cúbicas ejecutadas por el compositor. | Win32 |
| `gfx::draw` | Direct2D y DirectWrite: formas, texto, mapas de bits, efectos. | Win32 |
| `gfx::backdrop` | Acrylic por instantánea para barra, menú y tooltip. Mica o sólido para ventanas secundarias. | `gfx::draw`, `theme` |
| `theme` | Claro u oscuro, color de acento, "reducir movimiento". Escucha cambios del sistema y avisa a las ventanas. | Registro, Win32 |
| `model::entries` | Lista ordenada de entradas y `entries.json`. | `fsx` |
| `model::settings` | Ajustes tipados y `settings.json`. | `fsx` |
| `model::legacy_import` | Importación de BatLauncher en el primer arranque. | `model::entries`, `fsx` |
| `icons::cache` | `icons.cache`: mapas de bits BGRA premultiplicados por entrada y tamaño en píxeles. | `fsx` |
| `icons::extract` | Decodifica imágenes con WIC y extrae iconos de `.exe` y `.lnk` con el shell, en un hilo de trabajo. | Win32 |
| `launch` | Lanza entradas con ShellExecuteEx y abre la ubicación en el Explorador. | Win32 |
| `i18n` | Tablas de texto embebidas y función `t(clave)`. | — |
| `update::check` | Comprueba la última versión publicada. | WinHTTP |
| `update::apply` | Descarga, verifica y reemplaza el exe. | WinHTTP, `fsx` |
| `fsx` | Rutas de datos, escritura atómica, rotación de log. | — |
| `log` | Registro y captura de pánicos. | `fsx` |

Reglas de dependencia: `ui` depende de `gfx` y `model`; `model` no depende de nada de interfaz; `gfx` no conoce el modelo. Toda la lógica de layout, carrusel, versiones, importación y caché está en funciones puras testeables sin ventana.

### 6.2 Ciclo de vida del proceso

```
arranque
  │
  ├─ --updated <ruta.old> → borrar .old, continuar
  │
  ├─ ¿existe el mutex?  sí → enviar Glassbar.Toggle a la ventana de mensajes → salir
  │                     no → crear mutex y ventana de mensajes
  │
  ├─ cargar settings.json, entries.json (importando legacy si procede), icons.cache
  │
  ├─ --background → si residente está apagado: borrar clave Run y salir
  │                 si no: bandeja y atajo, sin mostrar la barra
  │
  └─ normal → crear ventana, capturar fondo, mostrar con animación

visible
  ├─ lanzar, Escape, perder foco, Toggle → ocultar
  ├─ menú y tooltip → la barra sigue visible
  └─ editar, ajustes → la barra se oculta; el proceso sigue mientras exista una ventana

oculto sin ventanas
  ├─ residente → esperar en bandeja: Toggle, atajo o bandeja vuelven a visible
  └─ no residente → salir
```

Cambiar "mantener en segundo plano" a apagado desde ajustes quita la bandeja y el atajo al momento; el proceso sale cuando se cierre la última ventana.

### 6.3 Ventanas y composición

Todas las ventanas de la app comparten el mismo mecanismo:

- Estilo `WS_POPUP`. Extendido `WS_EX_NOREDIRECTIONBITMAP`, de modo que no hay superficie GDI y todo el contenido son visuales de DirectComposition. Barra, menú y tooltip añaden `WS_EX_TOOLWINDOW` y `WS_EX_TOPMOST`; el tooltip además `WS_EX_NOACTIVATE`. Ajustes y diálogos añaden `WS_EX_APPWINDOW`.
- Un destino de composición por ventana con un visual raíz. Cada celda, control, texto y plato es un visual con una superficie dibujada una vez con Direct2D y redibujada solo cuando cambia su contenido.
- Las animaciones son propiedades animadas de DirectComposition: opacidad, desplazamiento y escala con curvas cúbicas. Las ejecuta el compositor de Windows; la app no interviene por frame.
- DPI por monitor V2 declarado en el manifiesto. Al cambiar de DPI se recalculan el layout y las superficies.
- La barra reserva un margen transparente de 32 px alrededor del plato para la sombra y el desplazamiento de apertura. `WM_NCHITTEST` devuelve transparente fuera del plato, así que los clics en el margen van a lo que haya debajo.

Acrylic por instantánea, en `gfx::backdrop`:

1. Antes de mostrar la ventana se calcula el rectángulo final del plato en píxeles físicos.
2. Se copia esa región de la pantalla con `BitBlt` desde el contexto de pantalla a un mapa de bits WIC. Coste de pocos milisegundos para una región de barra.
3. Con Direct2D se aplica desenfoque gaussiano de radio 20, tinte `rgba(40,40,46,0.62)` en oscuro o `rgba(249,249,249,0.70)` en claro, grano procedimental al 2 % y recorte al rectángulo redondeado. El borde de 1 px y la sombra se dibujan en la misma superficie, que es la superficie del plato.
4. Si la captura falla o devuelve una imagen negra, se usa un plato de tinte plano sin desenfoque.
5. En pantallas HDR la captura GDI puede salir oscura. Se detecta por el espacio de color de la salida DXGI y en ese caso se usa el plato plano.
6. Lo que hay detrás no se actualiza mientras la barra está visible. Es aceptable porque la barra se oculta al tocar cualquier otra ventana.

Ventanas secundarias: en Windows 11 22H2 o superior se activa el fondo Mica del sistema con esquinas redondeadas; el contenido se dibuja encima con zonas transparentes. En versiones anteriores, fondo sólido `#202024` en oscuro y `#F3F3F6` en claro. Estas ventanas no animan el plato; solo usan la transición estándar de DWM.

### 6.4 Hilos

| Hilo | Cuándo existe | Qué hace |
|---|---|---|
| Interfaz | Siempre | Bucle de mensajes, composición, entrada, lanzamiento. |
| Extracción de iconos | Mientras haya iconos sin caché | Decodifica y extrae; envía cada resultado a la interfaz con `PostMessage`. Al acabar escribe `icons.cache` y termina. |
| Actualización | Durante una comprobación o descarga | WinHTTP; envía progreso y resultado con `PostMessage`. |

Ningún temporizador permanente. Los únicos temporizadores son el del tooltip mientras el ratón está sobre una celda y el del parpadeo del cursor mientras un campo de texto tiene el foco. La inercia del carrusel es una animación del compositor: la app calcula el desplazamiento final y la curva, y el compositor la ejecuta.

### 6.5 Tema

Claro u oscuro desde `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`. Color de acento desde `DwmGetColorizationColor`. "Reducir movimiento" desde `SystemParametersInfo` con `SPI_GETCLIENTAREAANIMATION`. Los cambios llegan por `WM_SETTINGCHANGE` y `WM_DWMCOLORIZATIONCOLORCHANGED` a la ventana de mensajes, que avisa a las ventanas abiertas para redibujar.

### 6.6 Idiomas

`assets/strings/es.json` y `assets/strings/en.json`, embebidos en el binario. Claves con prefijo por pantalla, por ejemplo `menu.open`, `settings.language`. "Según el sistema" usa `GetUserDefaultUILanguage`: `es-*` da español y cualquier otro da inglés. Un test comprueba que todos los archivos tienen exactamente el mismo conjunto de claves y que ninguna cadena está vacía.

## 7. Datos

### 7.1 Carpeta de datos

```
%LOCALAPPDATA%\Glassbar\
├── entries.json
├── settings.json
├── icons\            imágenes copiadas y PNG extraídos, un archivo por entrada
├── icons.cache
├── update\           descargas temporales, se vacía al arrancar
└── log.txt           rotación a log.1.txt al superar 1 MB
```

Escritura atómica: se escribe `archivo.tmp` y se renombra sobre el destino. Un JSON ilegible se renombra a `archivo.corrupt-AAAAMMDD-HHMMSS` y se arranca con valores por defecto, dejando constancia en el log.

### 7.2 entries.json

```json
{
  "version": 1,
  "entries": [
    {
      "id": "6a215d29-2616-4b56-ab7a-d89e7c914296",
      "name": "ComfyUI",
      "path": "D:\\AI\\NComfyui\\UmeAiRT-Start-ComfyUI.bat",
      "icon": { "kind": "custom", "file": "6a215d29-2616-4b56-ab7a-d89e7c914296.png" },
      "created_at": "2024-11-14T07:57:00Z"
    }
  ]
}
```

- El orden del array es el orden en la barra.
- `icon.kind`: `custom` con `file` relativo a `icons\`, `file` para usar el icono del propio archivo, `none` para el glifo.
- Los campos desconocidos se conservan al reescribir, para no perder datos de versiones futuras.

### 7.3 settings.json

```json
{
  "version": 1,
  "language": "system",
  "resident": false,
  "start_with_windows": false,
  "hotkey": null,
  "theme": "system",
  "animations": true,
  "update_check": true,
  "last_update_check": null,
  "known_latest": null
}
```

`hotkey` es `null` o `{ "mods": ["win", "alt"], "key": "Space" }` con nombres de tecla virtual de Windows. `last_update_check` es fecha y hora UTC en ISO 8601. `known_latest` es la última versión vista en GitHub.

### 7.4 icons.cache

Archivo binario little-endian. Cabecera: magia `GBIC`, versión del formato `u16`, número de registros `u32`. Cada registro: `id` de 16 bytes, `huella` `u64`, `px` `u16`, y `px × px × 4` bytes BGRA premultiplicados. La huella se calcula con FNV-1a sobre la ruta de origen del icono, su fecha de modificación y su tamaño. El origen es la imagen copiada en `icons\` para `custom` y el propio archivo de la entrada para `file`; las entradas `none` no tienen registro. Un registro se usa si coinciden `id`, `huella` y `px`. Puede haber varios `px` por entrada para distintos monitores. Al escribir se eliminan los registros de entradas que ya no existen. Si la magia o la versión no coinciden, se ignora el archivo entero.

En el arranque se lee una sola vez, se crean los mapas de bits de Direct2D y se construyen los visuales antes del primer frame. Lo que falte se pide al hilo de extracción; esas celdas muestran el glifo y pasan al icono con un fundido de 150 ms al llegar.

### 7.5 Importación de BatLauncher

Se ejecuta si no existe `entries.json` y existe `%LOCALAPPDATA%\BatLauncher\data.json`.

| Origen | Destino |
|---|---|
| `id` | `id` |
| `name` | `name` |
| `filePath` | `path` |
| `iconPath` existente | se copia a `icons\<id>.<ext>`, `icon.kind = custom` |
| `iconPath` ausente o inexistente, archivo `.exe` | `icon.kind = file` |
| `iconPath` ausente o inexistente, resto | `icon.kind = none` |
| `createdAt` | `created_at` |

La carpeta de BatLauncher no se modifica. El resultado se escribe en el log. Si `entries.json` ya existe, nunca se vuelve a importar.

### 7.6 Registro

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, valor `Glassbar` con `"<ruta del exe>" --background`. Solo existe con "iniciar con Windows" encendido. Si el exe se mueve, el valor se reescribe en el siguiente arranque con residente e inicio activados.

## 8. Actualizaciones

Constantes de compilación: propietario `TermiSenpai`, repositorio `Glassbar`, nombre del activo `Glassbar.exe`, archivo de sumas `SHA256SUMS`.

Comprobación:

1. Petición `HEAD` a `https://github.com/TermiSenpai/Glassbar/releases/latest` con WinHTTP, HTTPS, sin seguir redirecciones, agente `Glassbar/<versión>`, tiempo de espera 5 s.
2. La cabecera `Location` termina en `/releases/tag/vX.Y.Z`. Se extrae la versión. Esta ruta no usa la API de GitHub y no tiene límite de peticiones. Las prerreleases no aparecen en `latest`.
3. Se compara como versión semántica con la compilada. Se guardan `last_update_check` y `known_latest`.
4. Automática: si el ajuste está encendido y han pasado más de 24 h, 1 s después del primer frame, en el hilo de actualización. Si el proceso sale antes de terminar, no pasa nada: se reintenta en el siguiente arranque.
5. Manual: botón "Buscar ahora".

Aplicación, solo al pulsar "Actualizar":

1. Descargar `releases/download/vX.Y.Z/Glassbar.exe` y `releases/download/vX.Y.Z/SHA256SUMS` a `update\`, con progreso.
2. Calcular SHA-256 del exe descargado y compararlo con la línea de `SHA256SUMS`. Si no coincide, borrar y mostrar "La descarga no supera la verificación".
3. Renombrar el exe en ejecución a `Glassbar.exe.old`, mover el nuevo a la ruta del exe, lanzar el nuevo con `--updated "<ruta .old>"` y salir.
4. El nuevo proceso borra el `.old`, con hasta 10 reintentos cada 200 ms, y abre ajustes con el estado "Actualizado a X.Y.Z".
5. Si la carpeta del exe no permite escribir, mostrar "No se puede escribir en la carpeta del programa" con el enlace a la release para descarga manual.

Publicación: `.github/workflows/release.yml` se ejecuta al subir una etiqueta `v*`. Comprueba que la etiqueta coincide con la versión de `Cargo.toml`, compila en release, genera `SHA256SUMS` y crea la release con los dos archivos y notas automáticas.

## 9. Rendimiento

Presupuestos, medidos en la máquina del usuario y anotados en `docs/perf.md` en cada release:

| Métrica | Objetivo |
|---|---|
| Primer frame en frío, bajo demanda, caché válida | menos de 50 ms desde el inicio del proceso |
| Segunda instancia: enviar Toggle y salir | menos de 15 ms |
| Memoria privada en residente, barra oculta | menos de 15 MB |
| CPU en residente, barra oculta | sin actividad: sin temporizadores ni sondeos |
| Tamaño del exe | menos de 2 MB |
| Captura y desenfoque del fondo | menos de 15 ms a 100 % de escala |

Cómo se consigue:

- Sin bitmap de redirección: el contenido va directo al compositor.
- Dispositivo WARP: evita cargar el driver de la GPU en cada arranque. El dibujo de superficies es mínimo y DWM compone y anima en la GPU.
- Una lectura de `icons.cache` sustituye a decodificar cada imagen.
- Nada de WinRT. COM se inicializa solo para Direct2D, el shell y los selectores de archivos.
- Las ventanas de ajustes y diálogos se crean la primera vez que se abren.
- Perfil de release con `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` y símbolos eliminados.
- Con `--perf` el proceso escribe en el log los tiempos de cada fase del arranque para poder comparar entre versiones.

## 10. Errores y recuperación

| Situación | Comportamiento |
|---|---|
| El archivo de una entrada no existe | Celda atenuada con punto rojo y tooltip. El menú contextual sigue permitiendo editar y quitar. |
| JSON corrupto | Se renombra a `.corrupt-fecha`, se arranca con valores por defecto, se anota en el log. Sin `entries.json` se vuelve a intentar la importación legacy. |
| Caché de iconos inválida, de otra versión o con huellas distintas | Se ignora lo que no coincide y se regenera en segundo plano. Nunca retrasa el primer frame. |
| Captura del fondo fallida, negra o en HDR | Plato de tinte plano sin desenfoque. Misma geometría y animaciones. |
| Mica no disponible | Fondo sólido en ventanas secundarias. |
| Dispositivo gráfico perdido | Se recrean dispositivo, superficies y visuales. Si falla dos veces seguidas, se anota en el log y el proceso sale limpio. |
| Atajo rechazado por Windows | Mensaje en la tarjeta del atajo; el ajuste se guarda vacío. |
| Clave de inicio con Windows no escribible | Mensaje en la tarjeta; el interruptor vuelve a apagado. |
| Sin red al comprobar actualizaciones | Silencioso en automático. "Sin conexión" en manual. Nunca afecta a la barra. |
| Verificación SHA-256 fallida | Se borra la descarga y se avisa. El exe actual no se toca. |
| Reemplazo del exe bloqueado | Si el renombrado falla, no se mueve nada y se avisa con enlace a descarga manual. |
| Pánico interno | Se escribe el log con el mensaje y la ubicación, se destruyen las ventanas y el proceso sale. Nunca queda una ventana encima de todo. |

## 11. Pruebas

Automáticas con `cargo test`, sin crear ventanas:

- Layout: posiciones de celdas y controles fijos, altura, ancho del área de iconos con el tope del 80 %, activación del carrusel, recorte de desplazamiento, estado de las flechas, cálculo de inercia.
- Posición de la barra según monitor y lado de la barra de tareas.
- `entries.json` y `settings.json`: ida y vuelta, valores por defecto con campos ausentes, conservación de campos desconocidos, archivo corrupto.
- Importación legacy con un `data.json` de ejemplo y sus iconos, incluidos los casos de icono ausente y archivo `.exe`.
- `icons.cache`: codificar y decodificar, rechazo por magia o versión, invalidación por huella, poda de entradas eliminadas.
- Versiones: análisis de la cabecera `Location`, comparación semántica, análisis de `SHA256SUMS`.
- Atajos: formato y análisis de `hotkey`.
- Idiomas: mismo conjunto de claves en todos los archivos, sin cadenas vacías.
- Reglas de nombre e icono al añadir un archivo según extensión.

Manuales, en `docs/checklist.md`, se pasan antes de cada release:

- Spike inicial: captura y desenfoque, animación de apertura y cierre, tiempo de primer frame.
- DPI 100, 150 y 200 %, y mover la barra entre monitores con distinto DPI.
- Barra de tareas abajo, arriba y lateral; dos monitores; barra de tareas con ocultación automática.
- Tema claro y oscuro, cambio de tema con la barra abierta, "reducir movimiento".
- Segunda instancia desde el icono anclado con la barra visible y oculta.
- Modo residente: bandeja, atajo, inicio con Windows, apagar residente con la barra abierta.
- Arrastrar archivos admitidos y no admitidos. Menú contextual completo. Editar, mover y quitar.
- Carrusel con 25 entradas: arrastre, inercia, rueda, flechas y teclado.
- Actualización completa contra una release de prueba, incluida una suma incorrecta.

## 12. Construcción y distribución

- `Cargo.toml` en la raíz del repositorio. Edición 2024. Dependencias: `windows` con las características de Win32 UI, Graphics Direct2D, DirectWrite, DirectComposition, DXGI, Imaging, Shell, Dwm, Registry y WinHTTP; `serde` y `serde_json`. Dependencia de compilación para embeber `app.ico` y el manifiesto.
- Manifiesto: DPI por monitor V2, `longPathAware`, sistemas operativos compatibles Windows 10 y 11.
- `#![windows_subsystem = "windows"]`: sin consola.
- Salida: `target/release/Glassbar.exe`, único archivo, sin instalador. Se ancla a la barra de tareas con el menú contextual del exe.
- Versión en `Cargo.toml`; la app la expone en ajustes y en el agente de usuario.

Estructura del repositorio en `main`:

```
Glassbar\
├── src\
│   ├── main.rs
│   ├── app\            instance.rs, lifecycle.rs
│   ├── ui\             window.rs, toolbar.rs, menu.rs, widgets.rs, settings.rs, edit_entry.rs
│   ├── gfx\            compositor.rs, draw.rs, backdrop.rs
│   ├── model\          entries.rs, settings.rs, legacy_import.rs
│   ├── icons\          cache.rs, extract.rs
│   ├── update\         check.rs, apply.rs
│   ├── launch.rs, theme.rs, i18n.rs, fsx.rs, log.rs
├── assets\             app.ico, Glassbar.manifest, strings\es.json, strings\en.json
├── tests\fixtures\     data.json legacy e iconos de prueba, SHA256SUMS de ejemplo
├── .github\workflows\release.yml
├── docs\               superpowers\specs, perf.md, checklist.md
├── Cargo.toml, Cargo.lock, build.rs
├── README.md           en inglés
└── README.es.md        en español
```

Rama `legacy-wpf`: se crea desde el estado actual del árbol de trabajo, incluidos los cambios sin confirmar en `App.xaml.cs`, `DataService.cs`, `EditDialog` y `ToolbarWindow`, con un único commit que deja la app WPF tal como se usa hoy. Después, el primer commit del proyecto Rust en `main` elimina el código WPF de esa rama.

`.gitignore`: `target/`, `.superpowers/`, `.claude/`, archivos temporales.

## 13. Fases de implementación

Cada fase termina con sus tests en verde y la app ejecutable.

0. **Preparación y spike.** Primero se crea la rama `legacy-wpf` con el estado actual del árbol de trabajo, antes de cambiar nada en `main`. Después, el spike: ventana sin bitmap de redirección, captura y desenfoque del fondo, un visual con la animación de apertura, medición de primer frame. Decide si la instantánea cumple el presupuesto de 5 ms y el aspecto esperado. Si no, se vuelve a este documento antes de seguir.
1. **Modelo y datos.** `fsx`, `log`, `model::entries`, `model::settings`, `legacy_import`, `icons::cache`, `icons::extract`, `i18n`. Todo con tests. Sin ventana todavía.
2. **Barra estática.** `gfx`, `theme`, `ui::window`, `ui::toolbar` sin carrusel: layout, iconos desde la caché, hover, tooltip, lanzamiento, ocultar, posición y DPI.
3. **Movimiento.** Apertura y cierre, cascada, carrusel con arrastre, inercia, rueda y flechas, teclado.
4. **Gestión.** `ui::widgets`, `ui::menu`, `ui::edit_entry`, confirmación, arrastrar archivos, mover y quitar.
5. **Ajustes.** `ui::settings`, tema forzado, idioma, animaciones.
6. **Proceso.** `app::instance`, `app::lifecycle`, bandeja, atajo, inicio con Windows, `--background`.
7. **Actualizaciones.** `update::check`, `update::apply`, workflow de release, `--updated`.
8. **Cierre.** README en inglés y español, `docs/perf.md`, `docs/checklist.md`, limpieza del código WPF en `main`, release `v1.0.0`.

## 14. Riesgos

| Riesgo | Mitigación |
|---|---|
| La captura del fondo se ve distinta del Acrylic real o tarda más de 5 ms | Se valida en el spike con medición. Alternativa ya estudiada: Acrylic del sistema con el plato apareciendo sin desplazamiento. |
| Pantallas HDR | Detección por DXGI y plato plano. |
| El campo de texto propio no cubre un caso de escritura | Alcance acotado: una línea, cursor, selección, portapapeles, teclas muertas. La composición de métodos de entrada llega por la ventana de composición del sistema. |
| GitHub cambia el formato de la redirección de `latest` | El análisis está aislado y testeado con fixtures; el fallo solo afecta a la comprobación y se muestra como error en ajustes. |
| Antivirus bloquea el reemplazo del exe | El renombrado es el primer paso; si falla no se toca nada y se ofrece la descarga manual. |
| Un atajo global entra en conflicto con otra app | Windows lo rechaza y se informa; el usuario elige otro. |
| El exe se mueve de carpeta con inicio con Windows activo | La clave se reescribe en el siguiente arranque. |
