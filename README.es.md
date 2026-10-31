<div align="center">

<img src="apps/vulcan-code-web/public/vulcan-logo.png" alt="Vulcan Code" width="120" />

# Vulcan Code

**Un entorno de AI Coding de código abierto: aplicación de escritorio con GUI + TUI para la terminal**

Rust Core · Shell de escritorio Electron · React WebUI · ratatui TUI

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Issues](https://img.shields.io/github/issues/OpenVulcan/vulcan-code)](https://github.com/OpenVulcan/vulcan-code/issues)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/OpenVulcan/vulcan-code/pulls)

[简体中文](README.md) | [English](README.en.md) | [繁體中文](README.zh-TW.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | **Español** | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md) | [Português](README.pt-BR.md)

</div>

> Esta es una traducción del [README en chino simplificado](README.md). Si hay diferencias, prevalece la versión en chino. Las correcciones de traducción son bienvenidas mediante PR.

---

## Acerca del proyecto

Vulcan Code es una **aplicación de AI Coding** con dos clientes: una aplicación de escritorio con GUI y una TUI para la terminal. Reúne proyectos, sesiones, terminales, un navegador, revisión de código y commits de Git en un único entorno de trabajo, para que la IA entienda tu código, haga avanzar las tareas y entregue resultados que puedes revisar.

El proyecto se ha desarrollado y depurado internamente durante mucho tiempo y ahora se publica como código abierto bajo la **licencia MIT**. Queremos que sea algo más que el producto de un equipo: una herramienta que la comunidad perfeccione en conjunto.

- 🐛 ¿Encontraste un problema o tienes una idea? Abre un [Issue](https://github.com/OpenVulcan/vulcan-code/issues)
- 🔧 ¿Corregiste un error o creaste una función? Envía un [Pull Request](https://github.com/OpenVulcan/vulcan-code/pulls)
- 🤝 Tanto usuarios como desarrolladores pueden participar: construyamos juntos un software de AI Coding que a todos les encante

## Características destacadas

### 🧭 Entorno de escritorio

- Gestión de múltiples proyectos y sesiones, con paneles combinables de archivos, revisión, memoria y subagentes
- Terminal de proyecto integrada con pestañas; la IA puede mantener sesiones de comandos de larga duración o interactivas
- Respuestas en streaming con Markdown, bloques de código, diagramas Mermaid/SVG y tarjetas de llamadas a herramientas renderizadas en línea
- Edita una pregunta anterior para regenerar la respuesta o bifurca una nueva sesión desde cualquier turno completado
- Interfaz en chino e inglés, temas claro y oscuro

### 💻 Cliente de terminal (TUI)

- El mismo entorno de trabajo en la terminal: proyectos y sesiones, conversación y razonamiento, llamadas a herramientas, preguntas, cambios y commits de Git, además de memoria, base de conocimiento, skills, agentes, gestión de plugins y ajustes del sistema
- Comparte la misma interfaz de servicio que el WebUI y se conecta al mismo Core
- Soporte completo de ratón: clic, doble clic, menú contextual y rueda; paleta de comandos, comandos de barra y atajos de teclado
- Ábrelo desde el menú de la bandeja o compílalo e inícialo con `make bt`; consulta la [guía de la TUI](apps/vulcan-code-tui/README.md)

### 🎯 Avanzar con las tareas

- **Modo Goal**: define un objetivo y un presupuesto de tokens; la IA sigue trabajando turno a turno y puedes pausar, reanudar o borrar el objetivo
- **Modo Plan**: primero explora y elabora un plan, y lo ejecuta tras tu confirmación
- **Guía durante la ejecución**: añade instrucciones o pon en cola nuevas solicitudes mientras una tarea está en marcha
- **Subagentes**: roles integrados como Explore y Plan, además de más de 260 plantillas de roles de [agency-agents](https://github.com/msitarzewski/agency-agents) (desactivadas por defecto, actívalas según necesites)
- **Preguntas estructuradas**: cuando la IA necesita que tomes una decisión, te pregunta con tarjetas de opciones

### 🧠 Comprender el código y acumular experiencia

- **Navegación de código CodeKit**: localización estructural, búsqueda de texto, extracción del cuerpo de funciones y sustitución de funciones completas directamente sobre el árbol de trabajo en vivo; no hay índice precompilado, así que nada queda desactualizado. Conecta un servicio MCP externo cuando necesites búsqueda semántica de código
- **Base de conocimiento gestionada por la IA**: la IA organiza, lee y busca material a largo plazo con herramientas dedicadas, con búsqueda BM25, vectorial e híbrida; el historial se conserva para siempre y se puede restaurar; se autoriza por proyecto y se selecciona por sesión
- **Memoria a largo plazo MemoryMesh**: extrae experiencia al final de cada turno y la recupera antes del siguiente; las memorias se pueden ver, corregir y eliminar, con papelera y trazabilidad del origen

### 🔍 Resultados verificables

- **Revisión de diffs**: vistas unificada/dividida, diferencias a nivel de palabra, búsqueda de texto completo, staging y reversión
- **Commits de Git**: la IA genera el mensaje de commit; confirma en la rama actual o en una nueva y haz push directamente
- **Navegador de IA**: la IA navega, hace clic, toma capturas y lee la consola en una página real de Chromium; puedes tomar el control en cualquier momento

### 🧩 Extensible

- **Skills de prompts** (`SKILL.md`): documenta cómo trabaja tu equipo y reutilízalo la próxima vez
- **LuaSkills**: empaqueta acciones frecuentes en paquetes de capacidades programables, configurables e instalables
- **MCP**: servidores MCP de proceso local y HTTP, con diagnóstico de conexión e inspección de esquemas de herramientas
- **Servicio MCP**: Vulcan también es un servidor MCP. Otras herramientas de IA como Claude Code y Codex pueden, según lo concedido, listar proyectos y sesiones, enviar un mensaje y esperar la respuesta, iniciar sesiones, detener ejecuciones y responder preguntas, y llamar a modelos de imagen y vídeo concedidos aparte; cada herramienta tiene su propio token y las aprobaciones de permisos siempre se quedan en Vulcan
- **Sistema de plugins**: los plugins pueden aportar herramientas y montar su propia interfaz. Plugins integrados: búsqueda de documentación Context7, navegador de IA, estadísticas de uso y herramientas de depuración CDP/DAP

### ⚙️ Modelos y proveedores

- Compatible con las familias de protocolos OpenAI, Anthropic, Gemini y Cohere, con configuraciones integradas para OpenAI, Anthropic, Google, DeepSeek, Kimi, MiniMax, Alibaba, OpenRouter y otros; también puedes añadir proveedores personalizados
- Conversación, recuperación de memoria, vectores de la base de conocimiento y generación multimedia pueden usar cada una un modelo distinto
- Capacidades ampliadas: búsqueda y extracción web; comprensión de imágenes, vídeo y audio; generación de imágenes y vídeo; síntesis de voz y transcripción
- Políticas de ventana de contexto por capas, con compactación automática o manual (`/compact`)
- **Compresión de salida de comandos RTK**: RTK está integrado de forma nativa en el host y comprime la salida de comandos y el contexto de commits de Git para reducir el consumo de tokens; se activa en los ajustes, donde también se ven las estadísticas de ahorro
- Estadísticas de uso de tokens, latencia y coste de referencia

## Hoja de ruta

Hoy están disponibles la **aplicación de escritorio con GUI** y la **TUI para la terminal**. Próximamente:

- [ ] **Conexión remota desde el móvil**: ver y controlar desde el teléfono las sesiones que se ejecutan en tu escritorio
- [ ] **Verificación en Linux / macOS**: los scripts de compilación existen, falta una verificación completa en cada plataforma

Si alguno de estos puntos te interesa o tienes nuevas ideas, hablemos en los Issues.

## Arquitectura

```mermaid
flowchart LR
  subgraph Desktop["Shell de escritorio Electron (ventana / bandeja / host)"]
    WebUI["React WebUI"]
  end
  TUI["Cliente de terminal TUI (ratatui)"]
  Core["Rust Core (vulcan-code-service)<br/>núcleo de sesiones · runtime de agentes · herramientas<br/>protocolos de proveedores · MCP · LuaSkills<br/>base de conocimiento · MemoryMesh · RTK<br/>host de terminal · host de navegador · plugins"]
  WebUI <-->|"RPC / eventos"| Core
  TUI <-->|"mismo RPC / eventos"| Core
```

El servicio Core puede ejecutarse por sí solo en segundo plano (bandeja del sistema); cerrar la ventana de escritorio o salir de la TUI no interrumpe las tareas en curso. La aplicación de escritorio, el WebUI en un navegador y la TUI se conectan al mismo Core.

| Directorio | Descripción |
| --- | --- |
| `apps/vulcan-code-service` | Host del servicio Core: núcleo de sesiones, runtime de agentes y servicios RPC/streaming del WebUI |
| `apps/vulcan-code-desktop` | Shell de escritorio Electron: ventana y bandeja; inicia el Core local y carga el WebUI |
| `apps/vulcan-code-web` | WebUI React/Vite: entorno de trabajo, ajustes, sesiones, diff, skills y más |
| `apps/vulcan-code-tui` | Cliente de terminal TUI (ratatui): llama a la misma interfaz de servicio con las funciones y parámetros del WebUI |
| `crates/vulcan-agent-runtime` | Runtime compartido de agentes: proveedores, herramientas, LuaSkills, MCP, ejecución en streaming |
| `crates/vulcan-service-*` | API de servicios backend, núcleo del servicio Core y adaptadores de runtime |
| `crates/vulcan-memorymesh-*` | Memoria a largo plazo MemoryMesh |
| `crates/vulcan-runtime-assets` | Recursos de referencia: prompts del sistema, manifiestos de proveedores, plantillas de subagentes |
| `plugins/` | Plugins integrados |
| `docs/contracts/` | Documentos de contratos de desarrollo |

## Primeros pasos

> La verificación completa se ha realizado en **Windows x86_64**. Linux y macOS cuentan con el script `make.sh` correspondiente, pero aún no se han probado de principio a fin; se agradece ayuda para verificarlos.

### Requisitos

- `make`
- [Rustup](https://rustup.rs/): la versión de la toolchain está fijada en `rust-toolchain.toml` y se instala automáticamente
- PowerShell 7
- Node.js y pnpm

### Compilar y ejecutar

```bash
make init-system   # Primera vez: instala herramientas de Rust, dependencias del host y Node/pnpm gestionados
make sync          # Sincroniza configuración, skills del sistema, plugins y runtime del navegador en output/
make build         # Compila Core, WebUI y el shell de escritorio Electron
make run           # Inicia la aplicación de escritorio
```

También puedes usar `make br` para compilar y ejecutar en un solo paso; `make bt` hace una compilación completa, inicia el Core y abre la TUI, sin la aplicación de escritorio ni Vite. Para compilaciones de release, usa `make release`.

Consulta [`docs/contracts/development-environment.md`](docs/contracts/development-environment.md) para más detalles sobre el entorno.

### Desarrollo diario

El servidor de desarrollo Vite, el servicio Core y el shell Electron son independientes y pueden iniciarse y reiniciarse por separado:

```bash
make vite start      # Inicia el servidor de desarrollo Vite del WebUI (también status / stop)
make service         # Inicia solo el Core en segundo plano, sin abrir la ventana
make core-restart    # Solo cambios en el Core de Rust: compilación incremental y reemplazo en caliente (Windows Debug)
make sync            # Vuelve a sincronizar tras cambiar recursos, configuración, skills o plugins
```

- Si solo cambias el WebUI, mantén Vite y el shell de escritorio en marcha para obtener recarga en caliente
- En modo debug, `make run` / `make br` se aseguran de que Vite esté en ejecución
- No edites `output/` directamente; los recursos de referencia están en `crates/vulcan-runtime-assets/resources/`

## Cómo contribuir

Todas las contribuciones son bienvenidas: reportes de errores, documentación, mejoras de UX, nuevas plataformas, nuevos proveedores, plugins y skills…

**Abrir un Issue**

- Errores: incluye tu sistema operativo, los pasos para reproducirlo, el comportamiento esperado y el real, y registros o capturas relevantes
- Propuestas de funciones: describe el caso de uso y el problema que quieres resolver

**Enviar un PR**

1. Haz fork del repositorio y crea una rama a partir de `main`
2. Antes de hacer cambios, lee [`AGENTS.md`](AGENTS.md). Recoge las convenciones a largo plazo del repositorio; para áreas específicas, lee también los documentos correspondientes en [`docs/contracts/`](docs/contracts/)
3. Verifica localmente antes de enviar:

   ```bash
   # Rust: prioriza comprobaciones dirigidas a los crates modificados
   cargo check -p <crate> --all-targets
   cargo nextest run -p <crate>

   # WebUI
   pnpm --dir apps/vulcan-code-web run lint
   pnpm --dir apps/vulcan-code-web run test
   pnpm --dir apps/vulcan-code-web run build
   ```

4. En la descripción del PR, explica qué cambiaste, por qué y cómo lo verificaste

Algunas convenciones:

- Los nombres, descripciones y esquemas de herramientas orientados a la IA se escriben en inglés; los textos de la interfaz para el usuario se localizan
- La concurrencia de compilación y pruebas está limitada a 4 por defecto; no elimines este límite en los comandos de verificación
- Procura que cada PR aborde una sola cosa para facilitar la revisión

## Agradecimientos

Vulcan Code se apoya en muchos proyectos de código abierto excelentes. Gracias a sus autores y comunidades.

**Componentes y herramientas integrados**

- [RTK](https://github.com/rtk-ai/rtk): compresión de la salida de comandos para ahorrar tokens (integrado de forma nativa en el host)
- [Context7](https://github.com/upstash/context7): documentación de bibliotecas actualizada y específica por versión (plugin Context7)
- [Playwright](https://github.com/microsoft/playwright) / [Playwright MCP](https://github.com/microsoft/playwright-mcp): controlador y herramientas del navegador de IA
- [LuaSkills](https://github.com/LuaSkills/luaskills): runtime de LuaSkills
- [ripgrep](https://github.com/BurntSushi/ripgrep) y [ast-grep](https://github.com/ast-grep/ast-grep): herramientas incluidas de búsqueda de texto y de árbol sintáctico

**Escritorio e interfaz**

- [Electron](https://www.electronjs.org/): shell de escritorio
- [React](https://react.dev/) y [Vite](https://vite.dev/): framework y herramienta de compilación del WebUI
- [shadcn/ui](https://ui.shadcn.com/), [Radix UI](https://www.radix-ui.com/), [Tailwind CSS](https://tailwindcss.com/): componentes y estilos
- [xterm.js](https://xtermjs.org/): renderizado de la terminal
- [Shiki](https://shiki.style/): resaltado de sintaxis de código y diffs
- [Mermaid](https://mermaid.js.org/): renderizado de diagramas
- [Lucide](https://lucide.dev/), [Iconify](https://iconify.design/): iconos

**Cliente de terminal**

- [Ratatui](https://ratatui.rs/): framework de renderizado de la TUI

**Rust Core**

- [Tokio](https://tokio.rs/) y [Axum](https://github.com/tokio-rs/axum): runtime asíncrono y framework de servicios
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk): SDK oficial de MCP para Rust
- [LanceDB](https://github.com/lancedb/lancedb) y [Apache Arrow](https://arrow.apache.org/): índice vectorial y datos columnares
- [SQLite](https://sqlite.org/) ([rusqlite](https://github.com/rusqlite/rusqlite)): almacenamiento local de datos
- [jieba-rs](https://github.com/messense/jieba-rs): segmentación de texto en chino para la búsqueda de texto completo de la base de conocimiento
- [portable-pty](https://github.com/wezterm/wezterm) (WezTerm): pseudoterminal multiplataforma

**Referencias de diseño e implementación**

- [OpenAI Codex](https://github.com/openai/codex): referencia para el transporte del protocolo Responses, los reintentos y la compactación de contexto; la gramática y el analizador en streaming de Apply Patch están adaptados de Codex
- [cc-switch](https://github.com/farion1231/cc-switch): referencia para la conversión multiprotocolo, las llamadas a herramientas en streaming y los datos de precios de modelos
- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI): referencia para el protocolo Gemini y la conversión de compatibilidad
- [OpenCode](https://github.com/anomalyco/opencode): referencia para los catálogos de proveedores y los prompts del sistema
- [agency-agents](https://github.com/msitarzewski/agency-agents): origen de las plantillas de roles de subagentes
- [Answer me with HTML](https://github.com/QingYunA/answer-me-with-html): inspiración para respuestas HTML, borradores compactos y componentes de plantillas; el protocolo, el renderizador integrado y el visor aislado son propios y no dependen de su CLI

**Comunidad**

- [LINUX DO](https://linux.do/): gracias a la comunidad LINUX DO por su apoyo y sus debates

La lista completa de dependencias de terceros y sus licencias está en [`THIRD_PARTY_NOTICES.md`](crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md); también puedes consultarla en la aplicación en **Settings → About and licenses**.

## Licencia

[MIT](LICENSE) © 2026 OpenVulcan
