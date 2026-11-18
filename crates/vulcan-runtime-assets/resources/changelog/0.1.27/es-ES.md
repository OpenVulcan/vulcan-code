Vulcan Code 0.1.27 incorpora un inspector del contexto de las solicitudes, facilita el uso de habilidades y complementos bajo demanda y mejora la coordinación entre subagentes y los permisos de clientes MCP.

### Conversaciones y espacio de trabajo

```ui-checklist
ok | Revisar el contexto de la solicitud | Abre el contexto desde el menú de la conversación para revisar las instrucciones, los mensajes y las definiciones de herramientas de la última solicitud. Antes del primer envío, se muestra una vista previa del contexto que se prepararía.
ok | Continuar la tarea principal | La conversación principal puede procesar mensajes prioritarios y resultados mientras los subagentes siguen trabajando. Los resultados se incorporan a solicitudes posteriores.
ok | Posición de lectura y tiempo transcurrido | Las actualizaciones en directo conservan la posición al leer mensajes anteriores. Al reabrir la conversación se recupera a partir de los eventos de Core el tiempo transcurrido de la solicitud activa.
```

### Habilidades, complementos y MCP

```ui-checklist
ok | Mencionar habilidades y complementos | Escribe `@` para seleccionar una habilidad o ruta del proyecto, y `#` para seleccionar un complemento. Las sugerencias muestran nombres cortos.
ok | Cargar herramientas bajo demanda | Vulcan Browser, Context7 y la gestión de conexiones MCP de la conversación se pueden invocar bajo demanda. LuaSkills 0.9.1 permite activar ese modo para las herramientas de paquetes que incluyen una guía.
ok | Conceder permisos MCP por grupo | Configura por cliente los permisos de conversación, herramientas independientes y funciones multimodales. Puedes autorizar por separado las herramientas de archivos, shell, CodeKit y bases de conocimiento, y limitar su alcance a proyectos o bibliotecas.
```

### Instalación, modelos y comentarios

```ui-checklist
ok | Instalador independiente 0.2.18 | En la primera instalación se miden los tiempos de respuesta y se recomienda la ruta de descarga más rápida. Puedes cambiarla. LuaSkills usa el Hub y la fuente de descarga de GitHub de la región elegida.
ok | Diagnóstico de inicio de LuaSkills | La pantalla de inicio muestra el progreso de las dependencias en el idioma seleccionado e identifica las que no se pudieron instalar.
ok | Qwen4 Max | La versión de disponibilidad general de Qwen4 Max figura ahora en Alibaba Cloud Token Plan.
ok | Datos de contacto opcionales | Al enviar comentarios, puedes adjuntar y revisar tus datos de contacto. Las respuestas públicas están disponibles en el centro de mensajes de la aplicación.
```
