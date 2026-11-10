Vulcan Code 0.1.18 corrige errores de compactación del contexto y mejora los atajos de envío, el cierre del navegador y la estimación de costes.

### Contexto y conversaciones

```ui-checklist
ok | Compactación del contexto | La compactación manual usa el modelo seleccionado. Los errores como la falta de saldo o un fallo de autenticación detienen los reintentos automáticos. Si el modelo de resumen solicita una herramienta, su vista previa queda asociada a la solicitud de compactación correcta.
ok | Historial de reintentos | Los errores de los reintentos automáticos de una misma solicitud aparecen en una sola tarjeta que permite revisar cada intento.
ok | Avisos de caducidad de la memoria | La conversación indica cuándo ha caducado el contexto de memoria que se había adjuntado.
```

### Controles y ajustes

```ui-checklist
ok | Atajos para poner en cola o intercalar | Mientras hay una tarea en curso, puedes configurar Enter para poner un mensaje en cola o intercalarlo. Ctrl+Enter realiza la otra acción. En una conversación inactiva, el mensaje se envía de inmediato.
ok | Aprobación predeterminada y razonamiento | Puedes elegir el modo de aprobación para las conversaciones nuevas. Al volver a un modelo ya utilizado se recupera su nivel de razonamiento guardado.
ok | Cierre del navegador local | Al cerrar el panel del navegador del espacio de trabajo, se cierran al instante las páginas de la conversación actual y se liberan sus recursos.
```

### Proveedores y uso

```ui-checklist
ok | Avisos sobre proveedores | NodeAI aparece en la lista solo después de configurarlo. Los proveedores marcados como inestables muestran un aviso.
ok | Solicitudes por modelo | La lista de solicitudes recientes en las estadísticas de uso se puede filtrar por modelo.
ok | Tarifas por tramos y por proveedor | Las estimaciones de uso pueden aplicar precios por tramos de tokens. También puedes definir precios de un modelo para un proveedor concreto.
```

### Notas de versión

```ui-checklist
ok | Notas de versión | Las notas de 0.1.14 a 0.1.17 vuelven a agruparse en listas para facilitar su consulta.
```
