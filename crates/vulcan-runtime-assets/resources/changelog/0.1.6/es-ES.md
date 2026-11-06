Vulcan Code 0.1.6 corrige el reinicio tras una actualización y la reapertura de la aplicación en Linux, permite cerrar una ventana de escritorio cuyo inicio falló e incluye el instalador 0.2.8, que realiza toda la instalación en el instalador interactivo.

### Escritorio

```ui-checklist
ok | Inicio fallido | Si falla el inicio o la carga de la interfaz, la ventana muestra botones para recargar y cerrar en Windows y Linux, de modo que puede cerrarse
ok | Directorio de datos ocupado | Si otro proceso ocupa el directorio de datos y no responde, la página de inicio indica que termines los procesos vulcan-code-service y vuelvas a abrir la aplicación
```

### Linux

```ui-checklist
ok | Reinicio tras actualizar | Tras una actualización el Core se reinicia de inmediato; la aplicación de escritorio abre la nueva versión sin esperar dos minutos
ok | Reapertura | Abrir la aplicación mientras se está cerrando la vuelve a iniciar en lugar de no hacer nada, y los procesos auxiliares de la aplicación de escritorio ya no se quedan abiertos
ok | Icono del dock | La entrada de menú del instalador 0.2.8 coincide con la ventana de la aplicación, así que un icono fijado en el dock le pertenece; ejecutar una vez la actualización del instalador reescribe una entrada existente
```

### Correcciones

```ui-checklist
ok | Errores del proveedor | Cuando un proveedor informa de un error a mitad de una respuesta, se muestra su propio mensaje en lugar de un aviso de que un elemento de la respuesta seguía abierto (protocolos OpenAI Chat y Anthropic)
```

### Instalador

```ui-checklist
ok | Instalador interactivo | Después del asistente, la revisión, el progreso de la descarga y el resultado siguen en el instalador interactivo, en el idioma de la interfaz; las actualizaciones también lo usan
ok | Descarga exacta | La revisión muestra exactamente cuánto se va a descargar; los archivos que ya están no se cuentan de nuevo
ok | La próxima vez | La última página explica cómo cambiar, actualizar o desinstalar la instalación más adelante, y que el instalador se actualiza primero a sí mismo
ok | Cancelar | Volver atrás o cancelar en la revisión no cambia nada y termina sin error
ok | Scripts de instalación | Los scripts de instalación muestran el origen de la descarga y el progreso, y pasan a otro servidor tras 30 segundos sin datos
```
