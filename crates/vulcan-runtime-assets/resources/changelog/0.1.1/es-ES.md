Vulcan Code 0.1.1 añade actualizaciones dentro de la aplicación, una nueva forma de instalar y un servicio de usuario opcional.

### Actualizaciones dentro de la aplicación

```ui-checklist
ok | Comprobar y descargar | El servicio busca versiones nuevas, descarga solo los archivos que cambiaron y cambia de versión solo tras superar la verificación de firmas
ok | Reinicio al confirmar | La aplicación de escritorio, la WebUI del navegador y la TUI muestran las actualizaciones disponibles y reinician en la nueva versión cuando lo confirmas
ok | Vuelta atrás automática | Si una versión nueva no arranca, el lanzador vuelve a la anterior y la aplicación te avisa
ok | Actualizaciones completas | Cuando hace falta una instalación completa, la aplicación descarga y ejecuta el instalador y se vuelve a abrir al terminar
```

### Instalación y servicio

```ui-cards
Programa de instalación | El nuevo vulcan-code-setup instala por usuario en Windows, macOS y Linux, con aplicación de escritorio, cliente de terminal, entradas de menú y acceso directo en el escritorio opcionales
Servicio de usuario | El servicio puede iniciarse al iniciar sesión: entrada de inicio de Windows, LaunchAgent de macOS o servicio de usuario de systemd en Linux
Rango de puertos | Los puertos que usa el servicio se cambian en la configuración avanzada del programa de instalación
Cliente de terminal | Si no hay ningún servicio en marcha, la TUI inicia el servicio instalado, igual que la aplicación de escritorio
```

```ui-callout info Actualizar desde 0.1.0
0.1.0 aún no tiene actualizaciones dentro de la aplicación: vuelve a ejecutar el instalador o el comando de instalación para pasar a 0.1.1; las versiones posteriores se actualizan dentro de la aplicación.
```
