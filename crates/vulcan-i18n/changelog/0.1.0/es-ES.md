Vulcan Code 0.1.0 es la primera versión preliminar pública: un banco de trabajo gráfico con IA para programar y para las tareas de trabajo diarias. Proyectos, sesiones, terminales, el navegador, la revisión de código y los commits de Git están en un solo lugar.

```ui-kv
Versión | Preliminar
Clientes | Aplicación de escritorio, WebUI en el navegador, cliente de terminal (TUI)
Idiomas de la interfaz | 11
Paquetes | Incluyen Python, Node.js y componentes del navegador; se instalan con un solo comando
Licencia | MIT
```

### Novedades destacadas

````ui-tabs
### Ventana de compresión
Cada sesión puede cambiar su ventana de compresión en cualquier momento desde el indicador de capacidad del contexto en el área de entrada, así que una tarea larga nunca necesita una sesión nueva.

```ui-kv
Tamaños | Cinco niveles, por defecto 200K, 256K, 272K, 373K y 500K tokens, editables en Configuración
Seguir al modelo | Elige "Predeterminada (según el modelo)" para comprimir según la ventana del modelo actual
Alcance | Solo cambia cuándo se produce la compresión automática, no la ventana de contexto propia del modelo
Compresión manual | Ejecuta /compact en la sesión actual cuando quieras
```

### Tarjetas de herramientas y plegado
Cada tipo de herramienta tiene su propia tarjeta, así que ni siquiera muchas llamadas a herramientas entierran la respuesta.

```ui-checklist
ok | Tarjetas específicas | Archivos, comandos, terminales, subagentes, búsqueda, multimedia, preguntas, variables de entorno, cambios de configuración y más
ok | Tiempos y enlaces | Cada llamada muestra cuánto tardó; los archivos de una tarjeta se abren directamente en el panel de archivos
ok | Plegado a demanda | Abiertas mientras se ejecutan y plegadas al terminar; las llamadas consecutivas se agrupan bajo un solo resumen
ok | Cambia cuando quieras | Elige el valor predeterminado en Configuración, o cambia por sesión con "Plegado de herramientas" en el área de entrada o con /collapse
```

### Multimodal
En "Extensiones multimodales" cada capacidad recibe uno o varios modelos; "Capacidad ampliada" las activa por sesión.

```ui-cards
Web | Búsqueda web y extracción de páginas
Comprensión | Comprensión de imagen, vídeo y audio
Generación | Generación de imágenes y vídeo; las imágenes también pueden usar una suscripción de Codex
Voz y música | Síntesis y transcripción de voz, generación de música y versiones
```

### Variables de entorno
La gestión integrada de variables de entorno mantiene los secretos en tu equipo; la IA solo ve los nombres de las variables.

```ui-checklist
ok | Tres capas | Capas global, de proyecto y de sesión editadas en la interfaz; la página se puede mostrar en la barra lateral
ok | Valores ocultos | Los valores nunca se muestran a la IA ni en la interfaz, y los que aparecen en resultados de herramientas se enmascaran
ok | Se rellenan a petición | Cuando a la IA le falta una variable te la pide en un formulario, y el valor nunca pasa por la conversación
ok | Inyección automática | Los comandos y terminales reciben las variables de su capa; los servidores MCP usan las globales
```

### Modelos de subagentes
Los subagentes solo usan los modelos que permites.

```ui-kv
Dos niveles | La configuración del sistema es la predeterminada; cada sesión puede definir la suya
Marcado por defecto | El modelo que hace la llamada se incluye por defecto y se puede desmarcar
Fuera del rango | Se rechaza directamente, nunca se sustituye en silencio por otro modelo
Configuración de la sesión | Cámbialo cuando quieras en "Configuración de la sesión", debajo del cuadro de entrada, o vuelve al valor predeterminado del sistema con un clic
```
````

### Banco de trabajo

```ui-cards
Proyectos y sesiones | Muchos proyectos y sesiones en paralelo y en tu propio orden, con paneles de árbol de archivos, revisión, memoria y subagentes
Terminales del proyecto | Terminales con varias pestañas; la IA puede mantener sesiones de comandos largas o interactivas
Panel de archivos | Menciona archivos con @, arrástralos al área de entrada, abre rutas desde terminales y resultados, busca y ve a una línea
Respuestas enriquecidas | Markdown, código y diagramas Mermaid en streaming; las respuestas interactivas añaden listas de verificación, comparaciones, gráficos, flujos y más, y las tarjetas de decisión insertan o envían tu respuesta
Ramificar y reintentar | Edita una pregunta anterior para regenerarla o bifurca una sesión nueva desde cualquier turno terminado
Paleta de comandos | Atajos de teclado, búsqueda en la conversación y copia de una conversación como Markdown
Asistentes dedicados | Sesiones con un prompt base fijo y ajustes congelados para una tarea recurrente
```

### Sacar el trabajo adelante

```ui-cards
Modo de objetivo | Define un objetivo y un presupuesto de tokens; la IA sigue trabajando turno a turno y puedes pausar, reanudar o borrar el objetivo
Modo de planificación | Primero explora y redacta un plan; se ejecuta cuando lo apruebas
Orientación en curso | Añade indicaciones o pon en cola peticiones de seguimiento mientras se ejecuta una tarea
Subagentes | Roles integrados de exploración y planificación, más plantillas de rol opcionales
Preguntas | Cuando la IA necesita una decisión, pregunta con tarjetas de opciones dentro de la sesión
```

### Código, conocimiento y memoria

```ui-cards
Navegación de código | CodeKit lee el árbol de trabajo en vivo: símbolos, definiciones y referencias
Base de conocimiento | Bibliotecas que la IA mantiene por ti, con historial permanente y acceso por proyecto
Memoria | Memoria Markdown en tu carpeta de usuario, o memoria a largo plazo de MemoryMesh que se recupera antes de cada turno y se muestra en el proceso de trabajo
```

### Seguridad y revisión

```ui-checklist
ok | Modos de permisos | Solo lectura, escritura en el espacio de trabajo, aprobación automática y acceso total
ok | Cadena de aprobación | Reglas, un modelo de evaluación y un modelo revisor; todo lo dudoso pasa a ti
ok | Modelo de evaluación | Se ejecuta con un modelo local, en la nube o mediante una API
ok | Revisión de diff | Vista unificada y dividida, cambios por palabra, preparación y reversión
ok | Git | Pull, mensajes de commit escritos por la IA, commit en la rama actual o en una nueva y después push
```

### Ampliar Vulcan Code

```ui-cards
Habilidades | Habilidades de prompt (SKILL.md) y paquetes programables de LuaSkills, instalables desde LuaSkills Hub
MCP | Conecta servidores MCP locales y HTTP, o deja que la IA conecte uno solo para una sesión
Servicio MCP | Otras herramientas de IA pueden listar sesiones, enviar mensajes y responder preguntas, cada una con su propio token
Configuración de IA | La IA puede añadir servidores MCP y proveedores de modelos por ti, mostrando antes una vista previa y preguntándote según lo exija tu modo de permisos
Plugins | Los plugins añaden herramientas y páginas propias, como el navegador de IA y las estadísticas de uso
RTK | Comprime la salida de comandos y el contexto de Git para ahorrar tokens, y contabiliza el ahorro
Modelos | OpenAI, Anthropic, Gemini y otras familias de protocolos, proveedores integrados y personalizados, con modelos distintos para cada uso
```

```ui-callout info Versión preliminar
0.1.x sigue siendo una versión preliminar, y las funciones y la interfaz seguirán cambiando. Al actualizar, primero se hace una copia de seguridad de la base de datos de la aplicación y de los datos de sesión, y después se actualizan versión a versión automáticamente, sin que tengas que hacer nada.
```
