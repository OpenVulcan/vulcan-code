Vulcan Code 0.1.10 corrige la edición y la bifurcación en sesiones largas, mantiene los turnos ante límites de frecuencia y argumentos de herramienta mal formados, y mejora las tarjetas de comandos y los componentes de respuesta.

### Actualizar a esta versión

```ui-checklist
ok | Actualización incremental | Al actualizar desde 0.1.8 o 0.1.9 solo se descargan los archivos que cambiaron; las versiones anteriores siguen instalándose como actualización completa
ok | Instalaciones con el instalador | Las instalaciones hechas con el programa de instalación se actualizan con vulcan-code-setup update; la aplicación lo indica en lugar de ofrecer una descarga
```

### Sesiones

```ui-checklist
ok | Edición y bifurcación | En sesiones largas cuyo historial abarca varios segmentos, editar un mensaje enviado o bifurcar desde un turno ya no falla con el error provider_source fingerprint mismatch
ok | Argumentos de herramienta mal formados | Una llamada de herramienta con argumentos ilegibles recibe un resultado de error y el turno continúa, en vez de terminar al segundo error; los argumentos cortados se indican como probablemente demasiado largos
ok | Límites de frecuencia | Los límites HTTP 429 se reintentan según el calendario de límites de frecuencia cuando «Esperar y reintentar al alcanzar el límite de frecuencia» está activado en lugar de terminar el turno; un saldo agotado sigue deteniéndolo
```

### Herramientas y respuestas

```ui-checklist
ok | Títulos de comando | El argumento de descripción de shell_command pasa a ser un título corto de 80 caracteres como máximo, lo que evita que algunos modelos como MiniMax-M3.1-Flash rompan la llamada en ese argumento
ok | Tarjetas de trabajos en segundo plano | Las tarjetas de shell_job muestran el propósito del comando tras el identificador del trabajo, así se ve qué comando se está esperando; el cliente de terminal titula igual las filas de comandos y trabajos
ok | Componentes de respuesta | Una tercera celda en una fila ui-kv se muestra como nota bajo el valor, y un cambio con signo como +5 en ui-metrics como cambio, en vez de mostrar todo el componente como código
```

### Modelos y memoria

```ui-checklist
ok | OpenCode Go DeepSeek | deepseek-v4-pro y deepseek-v4.1-flash pueden desactivar el razonamiento, así que pueden elegirse como modelos de memoria
ok | Nota del modelo de memoria | Los ajustes explican ahora por qué solo aparecen modelos que pueden desactivar el razonamiento: el modelo de memoria se llama antes y después de cada respuesta, y el razonamiento sumaría espera y consumo cada vez
```
