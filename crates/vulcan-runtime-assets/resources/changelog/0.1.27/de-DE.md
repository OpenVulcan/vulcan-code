Vulcan Code 0.1.27 zeigt den Anfragekontext an, erleichtert den bedarfsgesteuerten Aufruf von Skills und Plug-ins und verbessert die Zusammenarbeit mit Subagenten sowie die Rechte für MCP-Clients.

### Unterhaltungen und Arbeitsbereich

```ui-checklist
ok | Anfragekontext prüfen | Öffne den Anfragekontext im Unterhaltungsmenü. Dort siehst du die Prompts, Nachrichten und Werkzeugdefinitionen der letzten Anfrage. Solange die Unterhaltung noch keine Anfrage gesendet hat, erscheint eine Vorschau des Kontexts.
ok | Hauptaufgabe fortsetzen | Die übergeordnete Unterhaltung kann priorisierte Nachrichten und eingehende Ergebnisse verarbeiten, während Subagenten noch laufen. Ergebnisse fließen in nachfolgende Anfragen ein.
ok | Scrollposition und verstrichene Zeit | Beim Lesen älterer Nachrichten halten laufende Aktualisierungen die Leseposition. Beim erneuten Öffnen der Unterhaltung stellt Core die verstrichene Zeit einer aktiven Anfrage anhand seiner Ereignisse wieder her.
```

### Skills, Plug-ins und MCP

```ui-checklist
ok | Skills und Plug-ins im Eingabefeld referenzieren | Mit `@` wählst du einen Skill oder Projektpfad aus, mit `#` ein Plug-in. Die Vorschläge zeigen kurze Namen.
ok | Werkzeuge bei Bedarf laden | Vulcan Browser, Context7 und die Verwaltung von MCP-Verbindungen einer Unterhaltung lassen sich bei Bedarf aufrufen. In LuaSkills 0.9.1 können Paketwerkzeuge mit Anleitung auf den Aufruf bei Bedarf umgestellt werden.
ok | MCP-Clientrechte nach Gruppen vergeben | Vergib Rechte für Unterhaltungen, eigenständige Werkzeuge und multimodale Funktionen pro Client. Datei-, Shell-, CodeKit- und Wissensdatenbankwerkzeuge können einzeln berechtigt und auf Projekte oder Bibliotheken beschränkt werden.
```

### Installation, Modelle und Feedback

```ui-checklist
ok | Eigenständiges Installationsprogramm 0.2.18 | Bei der ersten Installation misst das Programm die Antwortzeiten und schlägt die schnellere Downloadroute vor. Du kannst sie ändern. LuaSkills verwendet den Hub und die GitHub-Downloadquelle der gewählten Region.
ok | LuaSkills-Startdiagnose | Der Startbildschirm zeigt den Fortschritt beim Einrichten der Abhängigkeiten in der gewählten Sprache und nennt Abhängigkeiten, deren Installation fehlgeschlagen ist.
ok | Qwen4 Max | Die reguläre Version von Qwen4 Max ist jetzt für Konten mit Alibaba Cloud Token Plan gelistet.
ok | Optionale Kontaktdaten für Feedback | Beim Senden kannst du Kontaktdaten hinzufügen und vorher prüfen. Öffentliche Antworten findest du in der Nachrichtenzentrale der App.
```
