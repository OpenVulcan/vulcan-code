Vulcan Code 0.1.29 mantiene activas las llamadas largas a herramientas MCP externas y actualiza el catálogo de modelos de OpenAI.

### MCP externo

```ui-checklist
ok | Señales de actividad para llamadas largas | Si el cliente acepta SSE, las llamadas largas envían señales periódicas para reducir las desconexiones por inactividad. La misma solicitud recibe el resultado final cuando termina la herramienta.
ok | Comandos largos en segundo plano | Los plazos de espera del cliente siguen vigentes. Si una solicitud de Shell en primer plano se desconecta, el servicio detiene su árbol de procesos. Inicia los comandos largos con `background: true` y consulta el resultado con `shell_job` y el mismo `sid`. Se necesita el permiso `shell_job`.
ok | Diagnóstico de tokens más claro | El registro del servicio distingue las cabeceras de autenticación ausentes o mal formadas de los tokens no válidos y registra las rotaciones manuales. Las entradas solo incluyen la categoría del diagnóstico o el ID del cliente.
```

### Modelos

```ui-checklist
ok | GPT-6.2 Sol y Astra | Ambos modelos están disponibles en la API de OpenAI y en las suscripciones de Codex. Las suscripciones de Codex usan la ventana de contexto indicada en el catálogo de modelos.
ok | GPT Image 2.6 | Flare y Sunburst ya aparecen en el selector multimedia de OpenAI.
```
