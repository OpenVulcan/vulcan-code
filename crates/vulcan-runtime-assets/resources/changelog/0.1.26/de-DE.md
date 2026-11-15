Mit Vulcan Code 0.1.26 lässt sich ein nicht erreichbarer Projektordner neu verknüpfen, ohne Sitzungen zu verlieren. Auch App-Updates und die Anzeige von Werkzeugergebnissen wurden verbessert.

### Projekte und Sitzungen

```ui-checklist
ok | Projektordner wiederherstellen | Wurde ein Ordner verschoben oder ein Laufwerk getrennt, lässt sich der ursprüngliche Speicherort erneut prüfen oder ein neuer Ordner auswählen. Der Sitzungsverlauf bleibt erhalten. Der Wechsel erscheint im Gesprächsverlauf, und die nächste Modellanfrage verwendet den neuen Ordner.
ok | Eingehängte Laufwerke und eingegebene Pfade öffnen | Die Ordnerauswahl zeigt unter Linux und macOS eingehängte Volumes an. Einen eingegebenen Pfad können Sie öffnen, bevor Sie die Auswahl bestätigen.
ok | Beim Wechsel die neueste Nachricht öffnen | Wenn Sie zu einem anderen Gespräch wechseln, öffnet sich dessen neueste Nachricht zum Weiterlesen.
```

### Installation und Updates

```ui-checklist
ok | Updates vor dem Neustart vorbereiten | Automatische Downloads sind standardmäßig aktiv. Die App meldet, sobald ein Update heruntergeladen und geprüft wurde. Der Versionswechsel erfolgt nach der Bestätigung des Neustarts. Sie können automatische Downloads abschalten und Updates manuell starten.
ok | Eigenständiges Installationsprogramm 0.2.17 | Für Windows gibt es ein signiertes grafisches Installationsprogramm, für Linux AppImage-Pakete und für macOS ein DMG. Nach einer Terminalinstallation unter macOS öffnet `vcode manager gui` die grafische Verwaltung. Bei der Erstinstallation wählt LuaSkills passend zum Installationsweg den internationalen Hub oder den Hub für Festlandchina.
```

### Browser- und Dokumentwerkzeuge

```ui-checklist
ok | Karten für Browseraktionen | Playwright-Karten zeigen Aktion, Ziel und Ergebnis etwa für Navigation, Klicks und Bildschirmfotos. Der mitgelieferte Vulcan Browser-Skill beschreibt zudem die Schritte zur Ausführung und Überprüfung.
ok | Dokumentwerkzeuge von Vulcan Work | Vulcan Code installiert Vulcan Work aus dem gewählten LuaSkills Hub. Die Werkzeuge unterstützen PDF, Word, Excel und PowerPoint. Eigene Karten zeigen je nach Vorgang Dateien, Tabellenbereiche oder Ergebnisse. Die Hilfe für implizit registrierte LuaSkills-Werkzeuge zeigt, welche Parameter das jeweilige Werkzeug akzeptiert.
```
