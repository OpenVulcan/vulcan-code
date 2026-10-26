Vulcan Code 0.1.0 es la primera versión preliminar pública: un banco de trabajo gráfico de programación con IA que reúne en un solo lugar proyectos, conversaciones, terminales, el navegador, la revisión de código y los commits de Git.

```ui-kv
Versión | Preliminar
Clientes | Aplicación de escritorio, WebUI en el navegador, cliente de terminal (TUI)
Idiomas de la interfaz | 11
Licencia | MIT
```

### Banco de trabajo

```ui-cards
Proyectos y conversaciones | Varios proyectos y conversaciones en paralelo, con paneles de árbol de archivos, revisión, memoria y subagentes
Terminales del proyecto | Terminales con varias pestañas; la IA puede mantener sesiones de comandos largas o interactivas
Respuestas enriquecidas | Markdown, código, diagramas Mermaid y tarjetas de herramientas en streaming; las respuestas interactivas añaden componentes visuales como hallazgos, comparaciones, gráficos y flujos
Ramificar y reintentar | Edita una pregunta anterior para regenerar la respuesta o crea una conversación nueva a partir de cualquier turno terminado
Asistentes dedicados | Conversaciones con un prompt base fijo y ajustes congelados para una tarea recurrente
```

### Sacar el trabajo adelante

```ui-cards
Modo objetivo | Define un objetivo y un presupuesto de Token; la IA sigue trabajando turno a turno y puedes pausarla, reanudarla o borrar el objetivo
Modo de planificación | Primero explora y redacta un plan; se ejecuta solo cuando lo apruebas
Guía en curso | Añade indicaciones o pon en cola nuevas peticiones mientras se ejecuta una tarea
Subagentes | Roles integrados de exploración y planificación, además de plantillas de rol opcionales, cada uno limitado a los modelos que permitas
Preguntas | Cuando la IA necesita una decisión, pregunta con tarjetas de opciones dentro de la conversación
```

### Código, conocimiento y memoria

```ui-cards
Navegación de código | CodeKit lee el árbol de trabajo en vivo: símbolos, definiciones y referencias
Base de conocimiento | Bibliotecas que la IA mantiene por ti, con historial permanente y acceso por proyecto
Memoria | Memoria en Markdown dentro de tu carpeta de usuario, o memoria a largo plazo de MemoryMesh que se recupera antes de cada turno
```

### Seguridad y revisión

```ui-checklist
ok | Modos de permiso | Solo lectura, escritura en el espacio de trabajo, aprobación automática y acceso total
ok | Cadena de aprobación | Reglas, un modelo de decisión y un modelo revisor; todo lo dudoso pasa a ti
ok | Revisión de Diff | Vista unificada y dividida, cambios por palabra, preparación y reversión
ok | Commits de Git | Mensajes escritos por la IA, commit en la rama actual o en una nueva y después push
ok | Secretos | Los valores de las variables de entorno nunca se muestran a la IA ni en la interfaz
```

### Ampliar Vulcan Code

```ui-cards
Habilidades | Habilidades de prompt (`SKILL.md`) y paquetes programables de LuaSkills
MCP | Conecta servidores MCP locales y HTTP, o deja que la IA conecte uno solo para una conversación
Servicio MCP | Otras herramientas de IA pueden listar conversaciones, enviar mensajes y responder preguntas, cada una con su propio Token
Complementos | Los complementos añaden herramientas y páginas propias, como el navegador de IA y las estadísticas de uso
Modelos | OpenAI, Anthropic, Gemini y otras familias de protocolos, proveedores integrados y personalizados, con modelos distintos para cada uso
```

```ui-callout info Versión preliminar
Los formatos de datos guardados aún pueden cambiar entre compilaciones 0.1.x. Las actualizaciones que migran tus datos paso a paso empiezan con la 0.2.
```
