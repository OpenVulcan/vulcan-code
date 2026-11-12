Vulcan Code 0.1.22 incorpora memoria gratuita en la nube y mejora el envío de imágenes en conversaciones largas, los avisos de error y la grabación de voz.

### Memoria

```ui-checklist
ok | Memoria gratuita en la nube | Inicia sesión en OpenVulcan en un sitio compatible. Antes de activarla, revisa los modelos, las regiones de procesamiento y el tratamiento de los datos. El uso se muestra por etapa.
ok | Reconstrucción de vectores | Consulta los lotes completados y la cuenta atrás al alcanzar un límite de frecuencia. La reconstrucción se reanuda tras la espera.
ok | Memoria Markdown | Los conflictos distinguen la revisión provisional del lote de la última revisión guardada.
```

### Conversaciones

```ui-checklist
ok | Imágenes nuevas en conversaciones largas | Con la compactación automática activa, las imágenes anteriores pueden resumirse por lotes para enviar las nuevas en la solicitud original.
ok | Errores de proveedores | Los errores de cuota de Bailian se tratan como límites de frecuencia. Los fallos de saldo y de red aparecen en tarjetas de error. Los avisos habituales del sistema aparecen como filas de estado.
```

### Entrada de voz y herramientas

```ui-checklist
ok | Duración de grabación | Las grabaciones que alcanzan el límite de tiempo pueden enviarse a transcribir.
ok | Estadísticas de RTK | La clasificación de comandos usa la base de datos de RTK existente y omite el análisis de los registros de conversación de Claude.
```
