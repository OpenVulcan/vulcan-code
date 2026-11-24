Vulcan Code 0.1.30 führt den Sparmodus, die Anmeldung mit dem ChatGPT-Tarif und den Konfigurationstransfer ein. MCP, der integrierte Browser und die Wiederaufnahme wartender Nachrichten wurden verbessert.

### Sitzungen und Kontext

```ui-checklist
ok | Sparmodus pro Sitzung | Für reguläre Sitzungen lässt sich ein Verlaufsfenster festlegen. Standardmäßig können ab 40 abgeschlossenen Runden die ältesten 20 geeigneten Runden aus späteren Modellanfragen entfallen. Der vollständige Verlauf bleibt sichtbar. Beim Aktivieren in einer bestehenden Sitzung kann eine zusätzliche Modellanfrage anfallen.
ok | Wartende Nachrichten nach einem Fehler senden | Sobald die fehlgeschlagene Runde abgeschlossen ist, lässt sich eine ausgewählte Nachricht mit ihren ursprünglichen Anhängen und Modelleinstellungen senden. Die Anfrageansicht zeigt Zusammenfassungen, den berücksichtigten Verlauf und den Ablauf von Erinnerungen.
```

### Konten und Modelle

```ui-checklist
ok | Getrennte OpenAI-Anmeldungen | OpenAI API, Codex und ChatGPT-Tarife lassen sich separat verbinden. Codex-Anmeldung und Kontingentabfrage funktionieren wieder über HTTP. Für den ChatGPT-Tarif wurden Anhänge und einige Streaming-Antworten korrigiert.
ok | Modelldienst-Einstellungen gezielt übertragen | Ausgewählte Anbieter-Zugangsdaten und benutzerdefinierte Anbieter lassen sich in eine passwortgeschützte `.vc_mcfg`-Datei exportieren. Konten mit rotierenden Tokens erfordern eine Anmeldung auf dem Zielgerät.
ok | Verbrauch und Bildbearbeitung | Der Verbrauch des StepFun Step Plan ist in beiden Regionen abrufbar. Die OpenAI API unterstützt Bildbearbeitung mit GPT Image 2 und 2.5, Codex unterstützt Referenzbilder mit GPT Image 2.
```

### Werkzeuge und Arbeitsbereich

```ui-checklist
ok | Projektbezogene MCP- und LuaSkills-Kürzel | MCP-Server lassen sich auf Projekte beschränken und bei Bedarf aufrufen. Im `#`-Menü des Eingabefelds stehen verfügbare MCP- und LuaSkills-Einträge zur Auswahl. Die Sitzungsberechtigungen gelten weiterhin.
ok | Browser direkt bedienen | Seiten im integrierten Browser lassen sich direkt bedienen. Lange Unterhaltungen behalten ihre Leseposition zuverlässiger. Befehlsanzeigen zeigen Laufzeit und Zeitlimit zusammen.
```

### Aktualisierung

```ui-checklist
ok | Automatische Datenaktualisierung | Beim Öffnen aktualisiert die App ihre Datenbank und ältere Sitzungsdateien und sichert die vorherigen Daten. Bestehende reguläre Sitzungen bleiben im Standardmodus, bis der Sparmodus aktiviert wird.
ok | Windows-Installation und erneuter Versuch | Setup 0.2.19 installiert die Visual C++-Laufzeit (x64), wenn sie fehlt. Nach einer fehlgeschlagenen Startprüfung bleiben geprüfte Dateien erhalten. Beim erneuten Versuch werden nur fehlende oder beschädigte Inhalte geladen.
```
