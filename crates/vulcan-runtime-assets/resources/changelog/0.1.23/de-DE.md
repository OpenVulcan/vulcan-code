Vulcan Code 0.1.23 ergänzt die geschützte Verlaufsbereinigung, verbessert den integrierten Browser und korrigiert das Laden mitgelieferter Lua-Module unter macOS.

### Gespräche

```ui-checklist
ok | Verlauf schnell bereinigen | Wählen Sie in einem inaktiven, bereits komprimierten Standardgespräch über das Plus-Menü oder `/clean-history`, bis zu welchem Schritt bereinigt wird. Ältere Werkzeugdetails und Denkprotokolle werden entfernt. Vereinfachte Fragen und Antworten bleiben sichtbar, und der noch benötigte Verlauf ist geschützt.
ok | Scrollen in langen Gesprächen | Gestreamtes Markdown wird unabhängig aktualisiert. Beim Laden früherer Schritte und bereinigter Einträge bleibt die Leseposition erhalten.
```

### Integrierter Browser

```ui-checklist
ok | Seitendarstellung | WebUI rendert die Seitenstruktur des integrierten Browsers anstelle des bisherigen Screenshot-Streams.
ok | Größe des Ansichtsbereichs | Wählen Sie Desktop, Hochformat, Querformat oder eine eigene Größe. Sie können den Ansichtsbereich auch einmalig an das aktuelle Fenster anpassen.
```

### Arbeitsbereich und macOS

```ui-checklist
ok | Wissensdatenbank | Neue Seitenleisten heften die Wissensdatenbank standardmäßig an. Gespeicherte Anordnungen haben Vorrang.
ok | Lua-Laufzeit unter macOS | Das Laden mitgelieferter nativer Lua-Module wurde für Intel-Macs und Macs mit Apple-Chip korrigiert. Context7 initialisiert den lokalen Speicher beim ersten Öffnen der Einstellungen.
```
