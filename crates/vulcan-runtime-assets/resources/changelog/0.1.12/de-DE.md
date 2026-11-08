Vulcan Code 0.1.12 schließt Updates, die eine Neuinstallation brauchen, in der App ab und behebt die Fensterknöpfe und das Symbol unter macOS sowie Titel- und Prüfanfragen, die einige Anbieter abgelehnt haben.

### Aktualisierung auf diese Version

```ui-checklist
ok | Inkrementelles Update | Beim Update von 0.1.8 bis 0.1.11 werden nur geänderte Dateien heruntergeladen; ältere Versionen werden weiterhin vollständig installiert
ok | Neuinstallation in der App | Künftige Updates, die eine Neuinstallation brauchen, schicken Sie nicht mehr auf die Website: Nur geänderte Dateien werden heruntergeladen, installierte Grundressourcen wie Browser, Electron und Node werden weiterverwendet, und das Setup-Programm schließt das Update beim Neustart ab
ok | Mit dem Windows-Installer installiert | Eine mit dem Windows-Installer installierte Kopie erhält solche Updates als kleines signiertes Update-Paket statt des vollständigen Installers
```

### Oberfläche

```ui-checklist
ok | Fensterknöpfe unter macOS | Nur die Fensterknöpfe des Systems bleiben, Minimieren, Maximieren und Schließen erscheinen nicht mehr doppelt, und der grüne Knopf bietet wieder das Kachel- und Vollbildmenü des Systems
ok | Symbol unter macOS | Das Symbol im Dock und bei geschlossener App ist so groß wie das anderer Apps
ok | Karten der Modelldienst-Konten | Karten haben eine Mindestbreite, die der Oberflächenskalierung folgt, sodass Kontingentzeilen nicht mehr auf mehrere Zeilen umbrechen
ok | Einrichtungsassistent | Sind Sie bei OpenVulcan angemeldet und ist kein Prüfmodell eingerichtet, wählt der Prüfschritt das Cloud-Prüfmodell vor; gespeichert wird erst nach Ihrer Bestätigung
```

### Modelle und Anbieter

```ui-checklist
ok | Korrigierte Anfragen | Sitzungstitel, die Modellprüfung in den Einstellungen und Prompt-Entwürfe für dedizierte Assistenten senden jetzt einen Cache-Bereich, sodass OpenCode Go nicht mehr 400 zurückgibt und Codex Responses Lite sie nicht mehr ablehnt
ok | Cloud-Plattformen | CommandCode, OpenCode Go, Open Design, OpenRouter und TokenRhythm gelten als Cloud-Plattformen; gemeinsame Modellangaben werden zuerst von offiziellen Anbietern, dann von Cloud-Plattformen, dann von Relays übernommen
ok | Kernmodellliste | Was ein Modell selbst kann und seine Grenzen stehen einmal in einer Kernmodellliste, die Anbieterregeln erben; die Statusseite der Anbietermanifeste kennzeichnet daraus geerbte Felder
```
