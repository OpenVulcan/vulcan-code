Vulcan Code 0.1.30 añade el modo de ahorro, el inicio de sesión con el plan ChatGPT y la transferencia de configuraciones. También mejora MCP, el navegador integrado y la recuperación de mensajes en cola.

### Sesiones y contexto

```ui-checklist
ok | Modo de ahorro por sesión | Configura una ventana de historial para las sesiones normales. Por defecto, al completarse 40 turnos, los 20 turnos aptos más antiguos pueden excluirse de las solicitudes posteriores. El historial completo sigue visible. Activar el modo en una sesión existente puede generar una solicitud adicional al modelo.
ok | Enviar mensajes en cola tras un error | Una vez cerrado el turno fallido, puedes enviar el mensaje seleccionado con sus adjuntos y ajustes de modelo originales. El inspector muestra resúmenes, historial conservado y caducidad de la memoria.
```

### Cuentas y modelos

```ui-checklist
ok | Accesos de OpenAI separados | Conecta por separado cuentas de OpenAI API, Codex y el plan ChatGPT. Codex vuelve a permitir el inicio de sesión y las consultas de cuota por HTTP. Se han corregido los adjuntos y algunas respuestas en streaming del plan ChatGPT.
ok | Transferencia selectiva de configuración | Exporta las credenciales y proveedores personalizados que elijas en un archivo `.vc_mcfg` protegido con contraseña. Las cuentas con tokens rotatorios requieren iniciar sesión en el dispositivo de destino.
ok | Uso y edición de imágenes | Consulta el consumo del Step Plan de StepFun en ambas regiones. Edita imágenes con GPT Image 2 o 2.5 desde la API de OpenAI, o usa imágenes de referencia con GPT Image 2 desde Codex.
```

### Herramientas y espacio de trabajo

```ui-checklist
ok | Accesos de MCP y LuaSkills por proyecto | Limita los servidores MCP a proyectos concretos y úsalos bajo demanda. Selecciona una opción disponible en el menú `#` del cuadro de entrada. Los permisos de sesión siguen vigentes.
ok | Interacción directa con el navegador | Interactúa directamente con las páginas del navegador integrado. Las conversaciones largas conservan mejor la posición de lectura y las tarjetas de comandos muestran el tiempo transcurrido junto al límite.
```

### Actualización

```ui-checklist
ok | Actualización automática de datos | La aplicación actualiza la base de datos y las sesiones antiguas al abrirlas y conserva una copia previa. Las sesiones normales existentes siguen en modo estándar hasta activar el modo de ahorro.
ok | Instalación y reintento en Windows | El instalador 0.2.19 instala el entorno de ejecución Visual C++ x64 si falta. Si falla la comprobación de inicio, conserva los archivos verificados y solo vuelve a descargar lo que falta o esté dañado.
```
