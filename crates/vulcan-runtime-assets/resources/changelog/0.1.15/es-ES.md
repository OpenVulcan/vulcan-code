### Correcciones de compactación del contexto

- La solicitud de resumen conserva todo el historial de la conversación, los resultados de las herramientas y el contenido multimedia nativo. Si la entrada completa supera el límite de contexto del modelo, la compactación informa del error y conserva el historial.
- Los turnos sin terminar que cumplen los requisitos se pueden compactar manualmente cuando no hay ninguna solicitud activa. Se conservan la tarea original y la entrada en cola para poder continuar después.

### Otras mejoras

- Los subagentes pueden usar un nivel de razonamiento compatible con su modelo de destino. Los resultados de los subagentes, la entrada en cola y el panel de mensajes pendientes se mantienen sincronizados.
- Las estadísticas conservan el intervalo seleccionado y muestran el uso por horas en intervalos cortos.
- Al volver atrás en la aplicación de escritorio, la pantalla de carga inicial permanece cerrada. Los avisos de archivos en la web se adaptan al ancho de la ventana, y los clientes web y de terminal muestran los resultados `ok` como comprobaciones correctas.
