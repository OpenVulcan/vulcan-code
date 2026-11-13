Vulcan Code 0.1.23 incorpora la limpieza protegida del historial, mejora el navegador integrado y corrige la carga de módulos Lua incluidos en Linux y macOS.

### Actualización a esta versión

```ui-checklist
ok | Instalador 0.2.12 | El script reutiliza el instalador existente, que se actualiza y reinicia automáticamente. En Linux y macOS, al activar la integración con PATH, los comandos quedan disponibles en terminales nuevas. La instalación por línea de comandos muestra los comandos del servicio y los puertos de WebUI.
```

### Conversaciones

```ui-checklist
ok | Limpieza rápida del historial | En una conversación normal e inactiva que ya se haya compactado, elige hasta qué turno limpiar desde el menú de añadir o con `/clean-history`. Se eliminan los detalles antiguos de herramientas y razonamiento. Las preguntas y respuestas simplificadas siguen en la cronología y el historial que aún necesita la conversación queda protegido.
ok | Desplazamiento en conversaciones largas | El Markdown en curso se actualiza de forma independiente. La cronología conserva la posición de lectura al cargar turnos anteriores, incluido el historial ya limpiado.
```

### Navegador integrado

```ui-checklist
ok | Renderizado de páginas | WebUI representa la estructura de la página del navegador integrado en lugar de transmitir capturas de pantalla.
ok | Tamaños de la ventana gráfica | Elige un ajuste de escritorio, vertical u horizontal, introduce un tamaño personalizado o ajústalo a la ventana actual con una sola acción.
```

### Espacio de trabajo y entorno de ejecución

```ui-checklist
ok | Base de conocimiento | Las nuevas barras laterales fijan la base de conocimiento por defecto. Se respeta la disposición guardada.
ok | Entorno Lua en macOS | Se corrigió la carga de los módulos nativos Lua incluidos en Mac con Intel y Apple Silicon. Context7 inicializa su almacenamiento local al abrir los ajustes por primera vez.
ok | Entorno Lua en Linux | Los módulos nativos Lua incluidos, como OpenSSL, ya se cargan en ARM64 y x64. Al empaquetar se comprueba la compatibilidad con glibc 2.35.
```
