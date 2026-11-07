Vulcan Code 0.1.9 corrige las sesiones largas que se quedaban minutos en una solicitud tras enviar un mensaje y no se podían detener, y corrige dos problemas de escritorio en Linux y macOS.

### Actualizar a esta versión

```ui-checklist
ok | Actualización incremental | Al actualizar desde 0.1.8 solo se descargan los archivos que cambiaron; las versiones anteriores siguen instalándose como actualización completa
ok | Instalaciones con el instalador | Las instalaciones hechas con el programa de instalación se actualizan con vulcan-code-setup update; la aplicación lo indica en lugar de ofrecer una descarga
```

### Sesiones

```ui-checklist
ok | Sin bloqueos | Las sesiones largas cuyo historial abarca varios segmentos, como las que tienen mucha salida de herramientas, ya no esperan minutos tras enviar un mensaje, y el botón de detener actúa al instante
ok | Tras reiniciar | Tras reiniciar la aplicación, las respuestas anteriores de esas sesiones se siguen enviando al modelo en su propio formato, razonamiento incluido, en lugar de como texto simple
```

### Linux y macOS

```ui-checklist
ok | Mover la ventana | En Linux, la barra de título vuelve a mover la ventana justo después de restaurarla desde maximizada
ok | Navegador integrado | En Linux y macOS, el navegador integrado instalado funciona
```
