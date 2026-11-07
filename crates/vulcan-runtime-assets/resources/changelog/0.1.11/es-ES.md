Vulcan Code 0.1.11 añade comentarios, un centro de mensajes y una forma de corregir problemas en tu propio dispositivo, y actualiza los modelos de OpenAI, TokenRhythm, OpenCode Go y OpenDesign.

### Actualizar a esta versión

```ui-checklist
ok | Actualización incremental | Al actualizar desde 0.1.8, 0.1.9 o 0.1.10 solo se descargan los archivos que cambiaron; las versiones anteriores siguen instalándose como actualización completa
ok | Base de datos de la aplicación | En el primer inicio se hace una copia de seguridad de la base de datos de la aplicación y luego se actualiza automáticamente a la nueva estructura, sin que tengas que hacer nada
```

### Comentarios y mensajes

```ui-checklist
ok | Enviar comentarios | Con la sesión iniciada, «Enviar comentarios» permite informar de un problema o hacer una sugerencia; antes de enviar se filtran en tu dispositivo los secretos, rutas, tu nombre de usuario, URL y datos personales, y ves exactamente el contenido que se enviará
ok | Informar desde una tarjeta de error | Cuando una conversación falla, su tarjeta de error ofrece informar con un clic; solo en fallos que Vulcan Code puede causar, no en inicio de sesión, saldo, límites de frecuencia, caídas del servicio, tiempos de espera, política de contenido ni cancelación
ok | Centro de mensajes | El «Centro de mensajes» muestra anuncios, mensajes personales y respuestas de los mantenedores a tus comentarios; los anuncios importantes también aparecen como banner
ok | Corregirlo en este dispositivo | Elige una carpeta y se prepara una copia de vulcan-code como proyecto, con una conversación que tiene la habilidad de corrección; no se sube ni publica nada hasta que confirmes el pull request en esa conversación
ok | Entradas de GitHub | El menú de la cuenta añade «Sugerir en GitHub (issue)» y «Crear un pull request en GitHub»
ok | Cliente de terminal | El cliente de terminal también tiene la página de comentarios
```

### Modelos y proveedores

```ui-checklist
ok | OpenAI | Ocho modelos de la API pública declaran salida estructurada estricta con json_schema; GPT-5.6 Sol/Terra/Luna, GPT-6 Sol/Luna y GPT-6.1 Terra pueden desactivar el razonamiento
ok | TokenRhythm | Se añaden DeepSeek V4.1 Flash 1017, LongCat 2.0, MiniMax M3.1 Flash y los modelos de prueba Egg-Alpha y Neohorse 1 9B, y se retiran minimax-m2.5 y mimo-v2.5-pro; se declara la salida estructurada, los modelos de Messages envían ahora la intensidad de razonamiento y pueden desactivarlo, DeepSeek V4 Pro 0813 y GLM-5.2 pasan a Responses y Kimi K2.6 a Chat
ok | OpenCode Go | Se añaden Gemini 4 Argon, Hy4 y los modelos gratuitos Beam, Iceman y Crouton; diez modelos declaran salida estructurada
ok | OpenDesign | GPT-6 Luna usa ahora Responses y Claude Haiku 5.5 Messages; se añaden deepseek-v4.1-flash-eco, hy4-preview, minimax-m3.1-flash y 14 modelos de chat Plus/Pro/Max; la generación de imágenes con GPT Image 2, Nano Banana 2/2 Lite y Seedream 5.0/5.0 Pro está disponible en los planes que la incluyen; los niveles de razonamiento se limitan a los que publica cada proveedor
ok | Modelos de memoria | Más modelos pueden desactivar el razonamiento y devolver salida estructurada, así que más de ellos pueden elegirse como modelos de memoria
```
