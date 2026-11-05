Vulcan Code 0.1.5 da a cada modelo su propia ventana de compactación predeterminada, inicia la aplicación de escritorio en una sola ventana y permite elegir cada cuánto se buscan actualizaciones.

### Ventana de contexto

```ui-checklist
ok | Predeterminado por modelo | Cada modelo de conversación habilitado puede tener su propia ventana de compactación predeterminada para sesiones nuevas: la predeterminada del sistema, seguir al modelo, un nivel fijo o un tamaño personalizado; las sesiones existentes no cambian
ok | Ventanas personalizadas | Un tamaño personalizado admite un número de tokens o K para miles (por ejemplo 235K), como mínimo 100K y como máximo la ventana del modelo; el selector de ventana junto al cuadro de entrada también puede cambiar a un tamaño personalizado
ok | Cambiar de modelo | Al cambiar de modelo en una sesión se ofrece la ventana predeterminada del modelo de destino y nada cambia hasta que confirmes; una ventana que el modelo de destino no admite pasa a seguir al modelo en lugar de impedir el cambio
ok | Interfaz de terminal | La configuración de la TUI también permite fijar la ventana predeterminada por modelo
```

### Actualizaciones

```ui-checklist
ok | Intervalo de búsqueda | Elige cada cuánto se buscan actualizaciones automáticamente: cada hora, cada 6 horas, cada 12 horas o cada día, cada 6 horas de forma predeterminada; el cambio se aplica sin reiniciar
ok | Página de actualización de software | La página de actualización de software en Configuración se ha reordenado: primero la versión actual con buscar y reparar, después la configuración de actualización y por último la actualización encontrada con las notas de cada versión que incluye, de la más reciente a la más antigua; el diálogo de actualización muestra la misma lista de versiones
ok | Reinicio en Linux | En Linux, un Core iniciado desde la aplicación de escritorio se reinicia tras una actualización en lugar de quedarse en «Reiniciando»
```

### Aplicación de escritorio

```ui-checklist
ok | Inicio en una ventana | Ya no se abre una ventana de inicio aparte; la ventana de trabajo muestra la pantalla de inicio con las etapas reales y cualquier fallo, y abre la interfaz en el mismo lugar cuando Core está listo
ok | Apariencia al iniciar | La pantalla de inicio usa el idioma y la apariencia de la última vez; el primer inicio sigue al sistema
ok | Icono de la aplicación | El dock de Linux y el Dock de macOS muestran el icono de Vulcan Code en lugar de uno genérico; abrir la aplicación de nuevo trae al frente la ventana que se está iniciando
```

### Correcciones

```ui-checklist
ok | Modelos de embeddings | Los modelos de embeddings con tamaño de salida ajustable (como Qwen3-Embedding-8B, de 32 a 4096 dimensiones) ya no rechazan todas las respuestas
ok | Componentes de respuesta | Los avisos sin tono y las filas de hallazgos con cuatro celdas ahora se muestran en lugar de quedar como bloques de código, tanto en la WebUI como en la TUI
```

### Instalador

```ui-checklist
ok | Instalación en una línea en macOS | Ejecutar el script de instalación mediante una tubería de curl en macOS ya puede leer el teclado
ok | Aplicación de macOS | La aplicación de macOS que crea el instalador tiene icono, y el instalador indica dónde está al terminar
ok | Entrada de menú de Linux | La entrada de menú de Linux corresponde a la ventana en ejecución, de modo que un icono anclado al dock abre la aplicación
```
