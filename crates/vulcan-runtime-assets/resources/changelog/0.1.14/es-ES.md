Vulcan Code 0.1.14 mejora el escritorio de macOS y los indicadores de sesiones sin leer, y corrige la compactación del contexto y las estadísticas de uso.

### Escritorio y sesiones

```ui-checklist
ok | Escritorio de macOS | El paquete de la aplicación, el Dock y la barra de menús usan el nombre y el icono de Vulcan Code. Los controles nativos de la ventana se sitúan en la barra de título, donde un doble clic alterna la pantalla completa nativa.
ok | Sesiones sin leer | Las sesiones en segundo plano muestran un punto de no leído cuando terminan y lo conservan tras reiniciar. La sesión seleccionada también lo muestra si termina con la ventana oculta o sin foco, hasta que vuelvas a interactuar con ella.
ok | Estado de la transmisión | La interfaz web cierra los paneles de razonamiento que siguen abiertos al terminar una solicitud o un turno y recupera el estado de la transmisión al reconectar.
ok | Llamadas a herramientas y tarjetas de decisión | Las secuencias largas caben en el encabezado existente. Los clientes web y de terminal reconocen las opciones recomendadas sin espacio tras el asterisco y las tarjetas que empiezan directamente por las opciones.
```

### Contexto y uso

```ui-checklist
ok | Compactación del contexto | Las solicitudes de compactación conservan la lista de herramientas del turno de origen, de modo que los proveedores compatibles con la caché de prompts puedan reutilizar el prefijo guardado.
ok | Estadísticas de uso | Se corrigen los totales de tokens y las estimaciones de coste de referencia, y se identifican las estimaciones basadas en datos parciales. Si las estadísticas cambian durante la lectura, el panel vuelve a consultar los datos una vez.
```
