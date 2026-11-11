Vulcan Code 0.1.20 incorpora la transcripción en la nube de OpenVulcan y las carpetas vinculadas al proyecto, y mejora la recuperación de turnos en cola y el diagnóstico de errores de proveedores.

### Entrada de voz y espacio de trabajo

```ui-checklist
ok | Transcripción en la nube | Inicia sesión en OpenVulcan en un sitio que ofrezca el servicio. En los ajustes del micrófono puedes elegir el modelo recomendado por el sitio, un modelo de la nube concreto o el modelo de reconocimiento de voz de un proveedor configurado.
ok | Atajo de grabación | Configura un atajo para la entrada de voz y elige el idioma de reconocimiento y las opciones compatibles con el modelo.
ok | Carpetas vinculadas al proyecto | Añade carpetas locales desde la página principal del proyecto para explorarlas en el panel de archivos y abrir sus archivos desde las conversaciones. Los repositorios Git vinculados aparecen en la lista de cambios del proyecto.
```

### Conversaciones y memoria

```ui-checklist
ok | Recuperación de turnos en cola | Los turnos guardados se reanudan con el identificador y los ajustes del envío original. Los objetivos de las conversaciones existentes también se inician una sola vez.
ok | Ramas de conversación | Crea una rama desde un turno completado mientras la conversación sigue en marcha. Las tarjetas de herramientas que permanecen en la conversación muestran el estado final al terminar la ejecución.
ok | Reconstrucción de vectores de memoria | Los lotes respetan el límite de entradas por solicitud del modelo de embeddings. La vista previa muestra el número de lotes que se utilizará al reconstruir.
```

### Modelos y diagnóstico de errores

```ui-checklist
ok | Detección de modelos del proveedor | Consulta los modelos de chat de la cuenta seleccionada de un proveedor integrado y rellena el formulario de nuevo modelo con tu selección.
ok | Modelos de Alibaba Cloud | Token Plan incorpora Qwen4.0 Max Preview. Las API de Model Studio de Pekín y Singapur incorporan los modelos Qwen3.8.
ok | Detalles de errores y reintentos | Las tarjetas muestran un diagnóstico breve y permiten copiar el JSON completo o añadir el error actual a la lista de reintentos. Edita las reglas individualmente con palabras clave o expresiones regulares.
```
