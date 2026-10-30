# Rendimiento

Mediciones en la máquina de desarrollo: Windows 11 26300, GPU dedicada, dos monitores, monitor principal 2560×1440 a 100 %.

## Spike de composición (examples/spike.rs), 2026-10-30

| Dispositivo de dibujo | first-frame | creación D3D11 | captura | plato (desenfoque + sombra) |
|---|---|---|---|---|
| Hardware, 1.ª ejecución | 720,6 ms | — | 6,7 ms | 25,3 ms |
| Hardware, 2.ª ejecución | 384,9 ms | 350,5 ms | 3,1 ms | 10,7 ms |
| Hardware, 3.ª ejecución, con vídeo reproduciéndose | 2654,6 ms | 2613,8 ms | 4,8 ms | 12,9 ms |
| WARP | 63,9 ms | 28,4 ms | 5,6 ms | 9,0 ms |

Conclusiones:

- La creación del dispositivo D3D11 por hardware carga e inicializa el driver de la GPU en cada proceso y es impredecible: entre 350 ms y 2,6 s. Inaceptable para un proceso bajo demanda.
- Con WARP el dibujo de las superficies va por CPU, pero son pocas y pequeñas: el plato completo con desenfoque gaussiano y sombra cuesta 9 ms. La composición y las animaciones las sigue ejecutando DWM en la GPU, porque DirectComposition anima en el compositor, no en el dispositivo de dibujo.
- Decisión: Glassbar crea siempre el dispositivo con `D3D_DRIVER_TYPE_WARP`. Lo que queda del primer frame es sobre todo la carga de `d3d10warp.dll`, `d2d1.dll` y `dcomp.dll`.
- Desviación estándar del desenfoque usada: 20 píxeles lógicos. Aspecto validado con captura de pantalla.
- Detalle de API: `IDCompositionVisual::AddVisual(visual, TRUE, NULL)` coloca el visual al fondo de la pila, no encima. Para apilar encima hay que pasar el visual de referencia.

## Binario real

| Fecha | Build | first-frame | segunda instancia | memoria residente | exe |
|---|---|---|---|---|---|
| (pendiente) | | | | | |
