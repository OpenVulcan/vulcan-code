Vulcan Code 0.1.7 permite abrir la app de escritorio de Linux desde el dock y el menú de aplicaciones en Ubuntu 23.10 y posteriores, hace más seguras las reinstalaciones y las actualizaciones en Windows, cierra varios fallos de seguridad y de pérdida de datos, e incluye el programa de instalación 0.2.9.

### Actualizar a esta versión

```ui-checklist
ok | Actualización completa | Esta versión cambia el instalador y la estructura de la instalación, así que se instala como actualización completa: en Windows la app descarga y ejecuta el instalador una vez
ok | Instalaciones con setup | Las instalaciones hechas con el programa de instalación se actualizan ejecutando vulcan-code-setup update; la app lo indica en lugar de ofrecer una descarga
```

### Linux

```ui-checklist
ok | Dock y menú de aplicaciones | En Ubuntu 23.10 y posteriores, setup 0.2.9 instala una vez un perfil de AppArmor, de modo que la app de escritorio se abre desde el dock y el menú de aplicaciones y no solo desde un icono del escritorio
ok | Perfil desde la app | Si falta el perfil o está desactualizado, la configuración de actualizaciones ofrece instalarlo pidiendo una vez la contraseña de administrador, o muestra el comando para ejecutarlo en una terminal
```

### Windows

```ui-checklist
ok | Reinstalación segura | Instalar en una carpeta sin una instalación anterior nunca elimina sus archivos; si la reinstalación de la misma versión no termina, se restaura la anterior
ok | Arranque de prueba | Abrir la app de escritorio mientras el Core anterior sigue en marcha ya no cuenta como un arranque fallido de la nueva versión ni la revierte
```

### Seguridad

```ui-checklist
ok | Servicio local | El Core solo acepta solicitudes dirigidas a un nombre de host de loopback, así que una página web que apunte su propia dirección a este equipo no puede leer el servicio
ok | Adjuntos | Los adjuntos SVG se abren en un sandbox y no pueden ejecutar scripts, y las rutas de los adjuntos no pueden salir de su carpeta
ok | Comprobación de solo lectura | Las herramientas de extensiones cuyo nombre contiene un verbo de escritura, las opciones de escritura de git branch y las opciones de ejecución de fd ya no se tratan como de solo lectura
ok | Secretos | Guardar un valor normal sobre una variable de entorno secreta la mantiene secreta
```

### Correcciones

```ui-checklist
ok | Skills y proyectos | Instalar un skill local desde su propia carpeta ya no lo elimina, y al eliminar un proyecto también se eliminan sus variables de proyecto
ok | Memoria | La memoria Markdown lee correctamente los títulos en chino, japonés y coreano
ok | Modo plan | Activar o desactivar el modo plan después de terminar un turno ya no falla
ok | Parámetros de herramientas | Los parámetros que remiten a definiciones compartidas, como listas de valores permitidos, siguen visibles y obligatorios
ok | Parámetros del modelo | Los parámetros decimales se envían como números, y la extracción de Tavily funciona solo con URL
ok | Edición de archivos | Insertar al final de un archivo conserva su última línea y las líneas en blanco finales
ok | WebUI | Las flechas en el mensaje de commit mueven el cursor, se muestran las líneas del diff que empiezan por --- o +++, y los ejemplos de código conservan sus enlaces de imagen tal como están escritos
```

### Cliente de terminal

```ui-checklist
ok | Mensajes de seguimiento | Se aceptan los mensajes a una sesión existente, y un mensaje que no se acepta vuelve al cuadro de entrada
ok | Git | Una conexión perdida ya no bloquea el commit ni la generación del mensaje hasta reiniciar
ok | Niveles de razonamiento | Cada nivel de razonamiento se envía tal como se eligió
```

### Programa de instalación

```ui-checklist
ok | Perfil de AppArmor | Setup 0.2.9 instala el perfil con sudo cuando el sistema lo necesita y deja el comando exacto en las notas si no se introduce la contraseña; la desinstalación lo elimina
```
