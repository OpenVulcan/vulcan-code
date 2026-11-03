Vulcan Code 0.1.0 ist die erste öffentliche Vorschau: eine grafische KI-Werkbank zum Programmieren und für alltägliche Arbeitsaufgaben. Projekte, Sitzungen, Terminals, Browser, Code-Review und Git-Commits sind an einem Ort vereint.

```ui-kv
Release | Vorschau
Clients | Desktop-App, Browser-WebUI, Terminal-Client (TUI)
Oberflächensprachen | 11
Pakete | Python, Node.js und Browser-Komponenten enthalten; Installation mit einem Befehl
Lizenz | MIT
```

### Highlights

````ui-tabs
### Komprimierungsfenster
Jede Sitzung kann ihr Komprimierungsfenster jederzeit über die Anzeige der Kontextkapazität im Eingabebereich wechseln, sodass auch eine lange Aufgabe nie eine neue Sitzung braucht.

```ui-kv
Größen | Fünf Stufen, standardmäßig 200K, 256K, 272K, 373K und 500K Tokens, in den Einstellungen anpassbar
Dem Modell folgen | Mit "Standard (Modell folgen)" wird nach dem Fenster des aktuellen Modells komprimiert
Wirkung | Ändert nur, wann die automatische Komprimierung einsetzt, nicht das Kontextfenster des Modells selbst
Manuelle Komprimierung | /compact jederzeit für die aktuelle Sitzung ausführen
```

### Tool-Karten und Einklappen
Jede Art von Tool hat eine eigene Karte, sodass selbst viele Tool-Aufrufe die Antwort nie verdecken.

```ui-checklist
ok | Eigene Karten | Dateien, Befehle, Terminals, Subagents, Suche, Medien, Rückfragen, Umgebungsvariablen, Konfigurationsänderungen und mehr
ok | Dauer und Links | Jeder Aufruf zeigt, wie lange er gedauert hat; Dateien auf einer Karte öffnen sich direkt im Dateibereich
ok | Einklappen nach Bedarf | Offen während der Ausführung, eingeklappt nach Abschluss; aufeinanderfolgende Aufrufe werden unter einer Zusammenfassung gruppiert
ok | Jederzeit umschaltbar | Standard in den Einstellungen wählen oder pro Sitzung mit "Tool-Einklappen" im Eingabebereich oder /collapse umschalten
```

### Multimodal
Unter "Multimodale Erweiterungen" erhält jede Fähigkeit ein oder mehrere Modelle; "Erweiterte Fähigkeiten" schaltet sie pro Sitzung ein.

```ui-cards
Web | Websuche und Extraktion von Seiteninhalten
Verstehen | Bild-, Video- und Audioverständnis
Erzeugen | Bild- und Videoerzeugung; Bilder können auch ein Codex-Abonnement nutzen
Sprache und Musik | Sprachsynthese und Transkription, Musikerzeugung und Cover
```

### Umgebungsvariablen
Die integrierte Verwaltung von Umgebungsvariablen hält Geheimnisse auf deinem Rechner; die KI sieht nur die Variablennamen.

```ui-checklist
ok | Drei Ebenen | Globale, Projekt- und Sitzungsebene, bearbeitet in der Oberfläche; die Seite lässt sich in der Seitenleiste anzeigen
ok | Werte bleiben verborgen | Werte werden weder der KI noch in der Oberfläche angezeigt, und Werte in Tool-Ergebnissen werden maskiert
ok | Auf Anfrage ausgefüllt | Fehlt der KI eine Variable, fragt sie dich per Formular, und der Wert läuft nie durch die Unterhaltung
ok | Automatisch eingefügt | Befehle und Terminals erhalten die Variablen ihrer Ebene; MCP-Server nutzen die globalen
```

### Subagent-Modelle
Subagents verwenden nur die Modelle, die du erlaubst.

```ui-kv
Zwei Ebenen | Die Systemeinstellung ist der Standard; jede Sitzung kann eine eigene festlegen
Standardmäßig ausgewählt | Das aufrufende Modell ist standardmäßig enthalten und kann abgewählt werden
Außerhalb des Bereichs | Wird direkt abgelehnt und nie stillschweigend durch ein anderes Modell ersetzt
Sitzungseinstellungen | Jederzeit unter Sitzungseinstellungen unterhalb des Eingabebereichs ändern oder mit einem Klick zum Systemstandard zurückkehren
```
````

### Arbeitsumgebung

```ui-cards
Projekte und Sitzungen | Viele Projekte und Sitzungen nebeneinander in deiner eigenen Reihenfolge, mit Bereichen für Dateibaum, Review, Gedächtnis und Subagents
Projektterminals | Terminals mit mehreren Tabs; die KI kann lang laufende oder interaktive Befehlssitzungen offen halten
Dateibereich | Dateien mit @ erwähnen, in den Eingabebereich ziehen, Pfade aus Terminals und Ausgaben öffnen, suchen und zu einer Zeile springen
Reichhaltige Antworten | Markdown, Code und Mermaid-Diagramme im Stream; interaktive Antworten ergänzen Checklisten, Vergleiche, Charts, Abläufe und mehr, und Entscheidungskarten fügen deine Antwort ein oder senden sie
Verzweigen und wiederholen | Eine frühere Frage bearbeiten und neu generieren lassen oder von jeder abgeschlossenen Runde eine neue Sitzung abzweigen
Befehlspalette | Tastenkürzel, Suche in der Unterhaltung und Kopieren einer Unterhaltung als Markdown
Dedizierte Assistenten | Sitzungen mit festem Basis-Prompt und eingefrorenen Einstellungen für eine wiederkehrende Aufgabe
```

### Arbeit erledigen

```ui-cards
Zielmodus | Ziel und Token-Budget festlegen; die KI arbeitet Runde für Runde weiter und lässt sich pausieren, fortsetzen oder zurücksetzen
Planmodus | Zuerst erkunden und einen Plan schreiben, ausgeführt wird erst nach deiner Freigabe
Steuerung | Hinweise ergänzen oder Folgeanfragen einreihen, während eine Aufgabe läuft
Subagents | Integrierte Rollen zum Erkunden und Planen sowie optionale Rollenvorlagen
Rückfragen | Braucht die KI eine Entscheidung, fragt sie mit Optionskarten direkt in der Sitzung
```

### Code, Wissen und Gedächtnis

```ui-cards
Code-Navigation | CodeKit liest den aktuellen Arbeitsbaum: Symbole, Definitionen und Referenzen
Wissensbasis | Sammlungen, die die KI für dich pflegt, mit dauerhafter Historie und Zugriff pro Projekt
Gedächtnis | Markdown-Gedächtnis in deinem Benutzerordner oder MemoryMesh-Langzeitgedächtnis, das vor jeder Runde abgerufen und im Arbeitsprozess angezeigt wird
```

### Sicherheit und Review

```ui-checklist
ok | Berechtigungsmodi | Nur lesen, Arbeitsbereich schreiben, Automatisch genehmigen und Vollzugriff
ok | Genehmigungskette | Regeln, ein Bewertungsmodell und ein Prüfmodell; alles Unklare geht an dich
ok | Bewertungsmodell | Läuft als lokales Modell, in der Cloud oder über eine API
ok | Diff-Review | Einheitliche und geteilte Ansicht, Änderungen auf Wortebene, Stagen und Zurücksetzen
ok | Git | Pull, von der KI verfasste Commit-Nachrichten, Commit auf den aktuellen oder einen neuen Branch, danach Push
```

### Vulcan Code erweitern

```ui-cards
Skills | Prompt-Skills (SKILL.md) und programmierbare LuaSkills-Pakete, installierbar aus dem LuaSkills Hub
MCP | Lokale und HTTP-MCP-Server verbinden oder die KI einen Server für eine einzelne Sitzung verbinden lassen
MCP-Dienst | Andere KI-Werkzeuge können Sitzungen auflisten, Nachrichten senden und Rückfragen beantworten, jedes mit eigenem Token
KI-Konfiguration | Die KI kann MCP-Server und Modellanbieter für dich hinzufügen, zeigt zuerst eine Vorschau und fragt dich, wie es dein Berechtigungsmodus verlangt
Plugins | Plugins ergänzen Tools und eigene Seiten, etwa den KI-Browser und Nutzungsstatistiken
RTK | Komprimiert Befehlsausgaben und Git-Kontext, um Tokens zu sparen, und zählt die Ersparnis
Modelle | OpenAI, Anthropic, Gemini und weitere Protokollfamilien, integrierte und eigene Anbieter, mit eigenen Modellen je Verwendungszweck
```

```ui-callout info Vorschauversion
0.1.x ist noch eine Vorschau; Funktionen und Oberfläche werden sich weiter ändern. Beim Upgrade werden App-Datenbank und Sitzungsdaten zuerst gesichert und dann automatisch Version für Version aktualisiert, ohne dass du etwas tun musst.
```
