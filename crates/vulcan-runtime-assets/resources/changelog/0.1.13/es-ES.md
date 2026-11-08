Vulcan Code 0.1.13 corrige el navegador integrado en los Mac con Intel, las notificaciones que dejaban de cerrarse solas y las instalaciones de entornos de ejecución que volvían a descargar los mismos archivos.

### Actualizar a esta versión

```ui-checklist
ok | Actualización incremental | Al actualizar desde 0.1.8 hasta 0.1.12 solo se descargan los archivos que cambiaron
```

### Interfaz

```ui-checklist
ok | Las notificaciones vuelven a cerrarse solas | Después de cerrar una notificación con su propio botón de cierre o de acción, todas las notificaciones posteriores permanecían en pantalla hasta cerrarlas a mano; ahora vuelven a desaparecer solas
ok | Navegador integrado en Mac con Intel | La versión para macOS Intel (x64) de 0.1.12 incluía el navegador para Apple silicon, que no funciona en Mac con Intel; esta versión incluye el navegador para Intel y ahora se comprueba que todos los programas de cada versión para macOS coincidan con su procesador
```

### Entornos de ejecución

```ui-checklist
ok | Se reutilizan las descargas | Al instalar o reinstalar uv, Node.js o pnpm se reutiliza un archivo descargado antes si sigue superando su suma de comprobación, en lugar de descargarlo de nuevo
```
