Vulcan Code 0.1.0 ist die erste öffentliche Vorschau: eine grafische KI-Programmierumgebung, die Projekte, Unterhaltungen, Terminals, Browser, Code-Review und Git-Commits an einem Ort vereint.

```ui-kv
Release | Vorschau
Clients | Desktop-App, Browser-WebUI, Terminal-Client (TUI)
Oberflächensprachen | 11
Lizenz | MIT
```

### Arbeitsumgebung

```ui-cards
Projekte und Unterhaltungen | Viele Projekte und Unterhaltungen nebeneinander, mit Dateibaum, Review, Gedächtnis und Subagent-Bereichen
Projektterminals | Terminals mit mehreren Tabs; die KI kann lang laufende oder interaktive Befehlssitzungen offen halten
Reichhaltige Antworten | Markdown, Code, Mermaid-Diagramme und Werkzeugkarten im Stream; interaktive Antworten ergänzen visuelle Komponenten wie Befunde, Vergleiche, Diagramme und Abläufe
Verzweigen und wiederholen | Eine frühere Frage bearbeiten und neu generieren lassen oder von jeder abgeschlossenen Runde eine neue Unterhaltung abzweigen
Dedizierte Assistenten | Unterhaltungen mit festem Basis-Prompt und eingefrorenen Einstellungen für eine wiederkehrende Aufgabe
```

### Arbeit erledigen

```ui-cards
Zielmodus | Ziel und Token-Budget festlegen; die KI arbeitet Runde für Runde weiter und lässt sich pausieren, fortsetzen oder zurücksetzen
Planmodus | Zuerst erkunden und einen Plan schreiben, ausgeführt wird erst nach deiner Freigabe
Steuerung während der Ausführung | Hinweise ergänzen oder Folgeanfragen einreihen, während eine Aufgabe läuft
Subagenten | Integrierte Rollen zum Erkunden und Planen sowie optionale Rollenvorlagen, jeweils auf die erlaubten Modelle beschränkt
Rückfragen | Braucht die KI eine Entscheidung, fragt sie mit Optionskarten direkt in der Unterhaltung
```

### Code, Wissen und Gedächtnis

```ui-cards
Code-Navigation | CodeKit liest den aktuellen Arbeitsbaum: Symbole, Definitionen und Referenzen
Wissensbasis | Sammlungen, die die KI für dich pflegt, mit dauerhafter Historie und Zugriff pro Projekt
Gedächtnis | Markdown-Gedächtnis im Benutzerordner oder MemoryMesh-Langzeitgedächtnis mit Abruf vor jeder Runde
```

### Sicherheit und Review

```ui-checklist
ok | Berechtigungsmodi | Nur lesen, Arbeitsbereich schreiben, automatisch genehmigen und Vollzugriff
ok | Genehmigungskette | Regeln, ein Entscheidungsmodell und ein Prüfmodell; alles Unklare geht an dich
ok | Diff-Review | Einheitliche und geteilte Ansicht, Änderungen auf Wortebene, Stagen und Zurücksetzen
ok | Git-Commits | Von der KI verfasste Nachrichten, Commit auf den aktuellen oder einen neuen Branch, danach Push
ok | Geheimnisse | Werte von Umgebungsvariablen werden weder der KI noch in der Oberfläche angezeigt
```

### Vulcan Code erweitern

```ui-cards
Skills | Prompt-Skills (`SKILL.md`) und programmierbare LuaSkills-Pakete
MCP | Lokale und HTTP-MCP-Server verbinden oder die KI einen Server für eine einzelne Unterhaltung verbinden lassen
MCP-Dienst | Andere KI-Werkzeuge können Unterhaltungen auflisten, Nachrichten senden und Rückfragen beantworten, jedes mit eigenem Token
Plugins | Plugins ergänzen Werkzeuge und eigene Seiten, etwa den KI-Browser und Nutzungsstatistiken
Modelle | OpenAI, Anthropic, Gemini und weitere Protokollfamilien, integrierte und eigene Anbieter, mit eigenen Modellen je Verwendungszweck
```

```ui-callout info Vorschauversion
Gespeicherte Datenformate können sich zwischen 0.1.x-Builds noch ändern. Upgrades, die deine Daten schrittweise migrieren, gibt es ab 0.2.
```
