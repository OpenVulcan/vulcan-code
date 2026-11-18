Vulcan Code 0.1.28 añade espacios de trabajo propios e historial de llamadas para clientes MCP externos, y actualiza al inicio las habilidades ROOT gestionadas que estén por debajo de la versión requerida.

### MCP externo

```ui-checklist
ok | Proyectos y sesiones propios | Los clientes externos pueden registrar directorios de trabajo y crear sus propias sesiones. Las herramientas de archivos, CodeKit, shell y bases de conocimiento usan el espacio de trabajo asociado a la sesión.
ok | Comandos en segundo plano y terminales | Con los permisos correspondientes, los clientes pueden ejecutar comandos en segundo plano, consultar o detener tareas y usar sesiones de terminal.
ok | Historial de llamadas opcional | Activa el registro por cliente para consultar argumentos, resultados y estado. El historial se puede filtrar por cliente, proyecto o sesión.
```

### Actualización

```ui-checklist
ok | Las llamadas MCP independientes requieren un ID de sesión | Registra un proyecto con `vc_mcp_project`, crea una sesión con `vc_mcp_session` y pasa su `sid` a las herramientas independientes. La sesión determina el directorio de trabajo.
```

### Habilidades y verificación para desarrollo

```ui-checklist
ok | Actualización de habilidades ROOT al iniciar | CodeKit, Vulcan Curl y Vulcan Work se actualizan desde su origen configurado si no alcanzan la versión mínima. Si falla la actualización, se conserva la versión utilizable y se vuelve a intentar en el siguiente inicio.
ok | Prueba de aceptación HTTP para políticas de VMM | `make test-vmm-http` comprueba por HTTP la interacción entre los controles de políticas de la WebUI y un servicio local.
```
