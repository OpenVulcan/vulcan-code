Vulcan Code 0.1.8 ejecuta los comandos de shell como trabajos que pueden seguir en segundo plano, añade una página de inicio del proyecto, permite que el icono de la bandeja abra el cliente que elijas y cambie el idioma, e incluye varios cientos de correcciones de una revisión completa antes de la publicación. Incluye el programa de instalación 0.2.10.

### Actualizar a esta versión

```ui-checklist
ok | Actualización completa | Esta versión vuelve a cambiar el instalador, así que se instala como actualización completa: en Windows la app descarga y ejecuta el instalador una vez
ok | Instalaciones con setup | Las instalaciones hechas con el programa de instalación se actualizan ejecutando vulcan-code-setup update; la app lo indica en lugar de ofrecer una descarga
```

### Comandos de shell

```ui-checklist
ok | Salida en directo | Una tarjeta de comando desplegada muestra las últimas líneas de un comando en curso, así que los comandos largos ya no parecen atascados
ok | Trabajos en segundo plano | Un comando puede ejecutarse en segundo plano; un mensaje enviado mientras se ejecuta lo pasa allí, y también se puede pasar desde la tarjeta del comando
ok | Trabajos terminados | Si un comando en segundo plano termina después de que la IA se haya detenido, la sesión continúa sola; el final aparece como una tarjeta de contexto con el nombre y el resultado del comando
ok | Sin límite de tamaño | La salida va a un registro rotativo en lugar de detener el comando a los 8 MiB; la IA recibe las líneas más recientes y puede leer las anteriores cuando las necesita
ok | Panel de actividad | El panel de sesión cuenta los subagentes y los comandos en segundo plano y abre una lista con sus registros; reiniciar para actualizar avisa de los trabajos en curso
ok | Siempre visible | Los recuentos aparecen en todas las sesiones, incluso sin Git, subagentes ni comandos en ejecución
```

### Proyectos

```ui-checklist
ok | Inicio del proyecto | El nombre del proyecto en la barra lateral abre una página del proyecto con cambio de nombre, acciones rápidas, estadísticas de sesiones, una lista de sesiones con búsqueda y todos los ajustes del proyecto
ok | Cambios sin guardar | Los cambios sin guardar en la memoria del proyecto se conservan al salir de la página del proyecto y volver
```

### Bandeja

```ui-checklist
ok | Doble clic | En Windows, un doble clic en el icono de la bandeja abre el cliente que elijas: la app de escritorio, la WebUI o el cliente de terminal; el menú pasa al botón derecho
ok | Idioma | Un menú de idioma en la bandeja cambia el idioma de la interfaz, y los clientes abiertos lo siguen
```

### Interfaz

```ui-checklist
ok | Ancho de lectura | Todos los anchos predefinidos de la conversación son mayores, y una columna más ancha ya no pliega la navegación en ventanas normales
ok | Indicador de estado | El indicador de estado queda ahora en el borde superior del cuadro de entrada en lugar de flotar sobre la conversación
ok | Temas oscuros | El texto seleccionado se ve en los temas oscuros
ok | Componentes de respuesta | Las claves largas ya no estrechan los valores en una columna fina, y las fórmulas en varios grupos antes de una leyenda se muestran bien
ok | Ejecuciones de herramientas | Las tarjetas de herramientas en curso dentro de una ejecución plegada se mantienen juntas
```

### Correcciones

```ui-checklist
ok | Modo plan | Solo un plan enviado espera confirmación, y se restaura tras reiniciar
ok | Respuestas vacías | Una respuesta que termina sin contenido se reintenta según tus ajustes de reintento; si el límite de salida la corta antes de responder, se indica
ok | Continuar una sesión | Enviar un mensaje después de ordenar el contexto ya no impide empezar
ok | Parámetros del modelo | Un parámetro definido para ciertos modelos no se envía a otros modelos
ok | MCP | Las listas de herramientas repartidas en varias páginas se leen completas, y las cabeceras que defines tienen prioridad sobre las de variables de entorno
ok | Instalador de Windows | Si falla el cambio a la nueva versión, el instalador explica el motivo y conserva los archivos
ok | Instalación dañada | Una app de escritorio con una instalación incompleta lo indica en lugar de iniciar el servicio equivocado
ok | Correcciones de la revisión | Varios cientos de correcciones menores en sesiones, almacenamiento, memoria, archivos, permisos, plugins, la WebUI y el cliente de terminal
```

### Seguridad

```ui-checklist
ok | Navegador integrado | El navegador integrado bloquea todas las formas de dirección local del puerto protegido del Core
ok | Errores del proveedor | Las claves de API se quitan de los errores del proveedor antes de acortarlos o mostrarlos
ok | Herramientas de plugins | Los argumentos que no encajan con los parámetros de una herramienta de plugin se rechazan antes de ejecutarla
```

### Cliente de terminal

```ui-checklist
ok | Actividad | Una pestaña de actividad, un contador en la barra de sesión y una vista de registro para los comandos en segundo plano, además de un aviso antes de reiniciar
ok | Correcciones | Los selectores, pegar en respuestas a preguntas, la selección con el ratón, los menús largos y los bloques de código funcionan bien
```

### Programa de instalación

```ui-checklist
ok | Reintento tras un fallo | Volver a ejecutar setup tras un cambio de versión fallido registra de nuevo el servicio
ok | Varias instalaciones | Desinstalar una instalación no toca los lanzadores, entradas de menú ni servicios de otra
```
