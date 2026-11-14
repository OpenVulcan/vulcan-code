Vulcan Code 0.1.24 mejora los archivos adjuntos y la recuperación al cambiar de modelo, y corrige problemas del navegador integrado y los registros de terminal.

### Conversaciones y entrada

```ui-checklist
ok | Arrastrar y pegar archivos | Se recuperó la vista previa de adjuntos. Si el cliente conoce la ruta original, los archivos que el modelo no admite como adjuntos pueden enviarse como referencias. Las referencias enviadas se despliegan aparte del mensaje.
ok | Cambio de modelo y recuperación | El cambio de modelo conserva el historial original. Si el razonamiento no se puede reproducir de forma segura, puedes copiar los mensajes visibles al borrador de una conversación nueva y revisarlos antes de enviar.
ok | Compactación y desplazamiento | La compactación en curso se muestra por separado. El texto nuevo permanece visible al seguir el final de la conversación.
```

### Navegador integrado

```ui-checklist
ok | Visualización y edición | Se corrigió la carga de estilos y recursos de ventanas emergentes y se mejoró la edición. El navegador integrado mantiene ocultas las contraseñas cuando la página cambia su visibilidad.
```

### Registros y proveedores

```ui-checklist
ok | Registros de terminal | Se mejoró la ocultación de valores secretos repartidos entre líneas o afectados por recortes. Las tareas en segundo plano pueden detenerse aunque el filtrado no esté disponible.
ok | Errores del proveedor | Las tarjetas conservan mensajes legibles y enlaces de ayuda, y ocultan las URL que contienen credenciales.
ok | Compatibilidad con proveedores | Token Plan de Alibaba Cloud Model Studio deja de intentar subidas temporales a OSS. Se corrigieron las solicitudes de la base de conocimiento que usan embeddings de OpenVulcan.
```
