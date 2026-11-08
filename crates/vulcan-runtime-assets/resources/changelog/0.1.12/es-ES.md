Vulcan Code 0.1.12 completa dentro de la aplicación las actualizaciones que requieren reinstalar, y corrige los botones de ventana y el icono en macOS y las solicitudes de título y validación que algunos proveedores rechazaban.

### Actualizar a esta versión

```ui-checklist
ok | Actualización incremental | Al actualizar desde 0.1.8 hasta 0.1.11 solo se descargan los archivos que cambiaron; las versiones anteriores siguen instalándose como actualización completa
ok | Reinstalación dentro de la aplicación | Las próximas actualizaciones que requieran reinstalar ya no te envían al sitio web: solo se descargan los archivos que cambiaron, se reutilizan los recursos básicos instalados como el navegador, Electron y Node, y el programa de instalación completa la actualización al reiniciar
ok | Copias instaladas con el instalador de Windows | Una copia instalada con el instalador de Windows recibe estas actualizaciones como un pequeño paquete de actualización firmado en lugar del instalador completo
```

### Interfaz

```ui-checklist
ok | Botones de ventana en macOS | Solo quedan los botones de ventana del sistema, así que minimizar, maximizar y cerrar ya no aparecen duplicados, y el botón verde vuelve a ofrecer el menú de mosaico y pantalla completa del sistema
ok | Icono en macOS | El icono en el Dock y con la aplicación cerrada tiene el mismo tamaño que el de otras aplicaciones
ok | Tarjetas de cuentas de servicios de modelos | Las tarjetas tienen un ancho mínimo que sigue la escala de la interfaz, así que las filas de cuota ya no se parten en varias líneas
ok | Asistente de configuración | Si has iniciado sesión en OpenVulcan y no hay ningún modelo evaluador configurado, el paso de evaluación preselecciona el evaluador en la nube; no se guarda nada hasta que confirmes
```

### Modelos y proveedores

```ui-checklist
ok | Solicitudes corregidas | Los títulos de sesión, la validación de modelos en los ajustes y la redacción de prompts de los asistentes dedicados envían ahora un ámbito de caché, así que OpenCode Go ya no devuelve 400 y Codex Responses Lite ya no las rechaza
ok | Plataformas en la nube | CommandCode, OpenCode Go, Open Design, OpenRouter y TokenRhythm se clasifican como plataformas en la nube; la información compartida de los modelos se toma primero de los proveedores oficiales, luego de las plataformas en la nube y después de los intermediarios
ok | Lista de modelos base | Lo que un modelo puede hacer y sus límites se registran una sola vez en una lista de modelos base que heredan las reglas de los proveedores; la página de estado de los manifiestos de proveedores marca los campos heredados de ella
```
