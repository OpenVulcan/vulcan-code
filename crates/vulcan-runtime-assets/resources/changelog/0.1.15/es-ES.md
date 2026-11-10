Vulcan Code 0.1.15 corrige la compactación para conservar la entrada completa y permite reanudar los turnos pendientes aptos tras compactarlos manualmente.

### Correcciones de compactación del contexto

```ui-checklist
ok | Entrada completa al compactar | La solicitud de resumen conserva todo el historial de la conversación, los resultados de las herramientas y el contenido multimedia nativo. Si la entrada completa supera el límite de contexto del modelo, la compactación informa del error y conserva el historial.
ok | Compactación manual de turnos pendientes | Los turnos sin terminar que cumplen los requisitos se pueden compactar manualmente cuando no hay ninguna solicitud activa. Se conservan la tarea original y la entrada en cola para poder continuar después.
ok | Mensajes en cola tras la compactación | Los mensajes en cola que eliges insertar durante la compactación manual se envían en el orden de selección cuando la compactación termina correctamente. Si la compactación falla, se detiene o el proceso se reinicia, los mensajes permanecen en cola.
```

### Otras mejoras

```ui-checklist
ok | Razonamiento de subagentes y cola de mensajes | Los subagentes pueden usar un nivel de razonamiento compatible con su modelo de destino. Los resultados de los subagentes, la entrada en cola y el panel de mensajes pendientes se mantienen sincronizados.
ok | Uso por hora | Las estadísticas conservan el intervalo seleccionado y muestran el uso por horas en intervalos cortos.
ok | Correcciones de escritorio e interfaz | Al volver atrás en la aplicación de escritorio, la pantalla de carga inicial permanece cerrada. Los avisos de archivos en la web se adaptan al ancho de la ventana, y los clientes web y de terminal muestran los resultados ok como comprobaciones correctas.
```
