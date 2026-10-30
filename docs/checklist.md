# Lista de comprobación manual

Se pasa con `target\release\Glassbar.exe` antes de cada release. Marcado el estado de la primera pasada (2026-10-30, Windows 11 26300, dos monitores a 100 %).

## Barra

- [x] Arranque en frío desde el icono anclado: la barra aparece sobre la barra de tareas del monitor del cursor con la animación de apertura.
- [x] Hover resalta la celda y el tooltip aparece a los 120 ms con el nombre.
- [x] Clic lanza la entrada y la barra se oculta.
- [x] Escape oculta; clic fuera oculta; el proceso sale si no es residente.
- [x] Entrada cuyo archivo no existe: icono atenuado con punto rojo, el clic no lanza nada.
- [x] Sin entradas: texto "Pulsa + o arrastra un archivo" con los botones de añadir y ajustes.
- [x] Tema claro y oscuro del sistema; el acento de Windows se usa en interruptores y glifos.
- [ ] "Reducir movimiento" de Windows: todo aparece sin animación. (pendiente de pasar a mano)
- [ ] DPI 150 % y 200 %, y mover la barra entre monitores con distinto DPI. (pendiente: los monitores de referencia están a 100 %)
- [ ] Barra de tareas arriba o lateral. (pendiente)

## Carrusel

- [x] Con 45 entradas en un monitor de 2560 px la barra se limita al 80 % y muestra flechas y bordes desvanecidos.
- [x] Arrastre con el ratón con inercia al soltar; un arrastre corto no cuenta como clic.
- [x] Rueda del ratón desplaza una celda.
- [x] Flechas del teclado recorren las celdas y el carrusel sigue al foco.

## Gestión

- [x] Clic derecho en un icono: menú de cristal con Abrir, Abrir ubicación, Editar, Mover, Quitar.
- [x] Clic derecho en el plato: Nueva entrada, Ajustes.
- [x] Botón + abre el diálogo de nueva entrada; la barra se oculta y el proceso sigue vivo hasta cerrarlo.
- [ ] Guardar una entrada nueva con icono personalizado desde una imagen y desde un exe. (pendiente de pasar a mano)
- [ ] Arrastrar archivos admitidos y no admitidos sobre la barra. (pendiente de pasar a mano)
- [ ] Quitar con confirmación borra el icono copiado. (pendiente de pasar a mano)

## Ajustes

- [x] El engranaje abre la ventana de ajustes sobre Mica; la barra se oculta.
- [x] Mantener en segundo plano: añade el icono de bandeja y activa Iniciar con Windows y el atajo.
- [x] Iniciar con Windows escribe `HKCU\...\Run\Glassbar` con `--background`; apagar segundo plano lo borra.
- [x] Atajo global: con Win + Alt + Espacio configurado y la app en segundo plano, la combinación abre la barra.
- [x] `--background` sin modo residente sale sin mostrar nada.
- [ ] Cambiar el idioma reconstruye la ventana con los textos nuevos. (pendiente de pasar a mano)
- [ ] Cambiar el tema aplica la paleta a la barra en la siguiente apertura. (pendiente de pasar a mano)

## Proceso

- [x] Segunda instancia: envía "alternar" y sale en menos de 15 ms (3,2 ms medidos).
- [x] Residente con la barra oculta: 24 MB privados y sin consumo de CPU en reposo.
- [x] Salir desde el menú de bandeja termina el proceso y quita el icono.

## Actualizaciones

- [x] Comprobación automática al arrancar: sin release publicada muestra "Error: HTTP 404" en ajustes y no afecta a la barra. Con release publicada, la redirección de `releases/latest` devuelve la etiqueta y el estado pasa a "Nueva versión disponible".
- [x] Contra una release real (v2.0.1): un exe de prueba con versión 1.5.0 detecta "Nueva versión 2.0.1 disponible".
- [x] Actualizar descarga `Glassbar.exe` y `SHA256SUMS`, verifica la suma, renombra el exe en uso a `.old`, coloca el nuevo (byte a byte igual al de la release) y lo relanza con `--updated`.
- [ ] Suma incorrecta: "La descarga no supera la verificación" y el exe actual no se toca. (pendiente: requiere publicar una release con suma manipulada)
