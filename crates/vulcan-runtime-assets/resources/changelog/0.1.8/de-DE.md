Vulcan Code 0.1.8 führt Shell-Befehle als Jobs aus, die im Hintergrund weiterlaufen können, bringt eine Projektstartseite, lässt das Tray-Symbol den gewählten Client öffnen und die Sprache wechseln und enthält mehrere hundert Korrekturen aus einer vollständigen Prüfung vor dem Release. Das Setup-Programm 0.2.10 ist enthalten.

### Update auf diese Version

```ui-checklist
ok | Vollständiges Update | Diese Version ändert den Installer erneut und wird daher als vollständiges Update installiert: Unter Windows lädt die App den Installer einmal herunter und führt ihn aus
ok | Setup-Installationen | Mit dem Setup-Programm erstellte Installationen werden mit vulcan-code-setup update aktualisiert; die App weist darauf hin, statt einen Download anzubieten
```

### Shell-Befehle

```ui-checklist
ok | Live-Ausgabe | Eine aufgeklappte Befehlskarte zeigt die neuesten Zeilen eines laufenden Befehls, sodass lange Befehle nicht mehr hängend wirken
ok | Hintergrund-Jobs | Ein Befehl kann im Hintergrund laufen; eine Nachricht, die während eines Befehls gesendet wird, verschiebt ihn dorthin, und auf der Befehlskarte lässt er sich auch von Hand verschieben
ok | Beendete Jobs | Endet ein Hintergrundbefehl, nachdem die KI angehalten hat, setzt die Sitzung von selbst fort; das Ende erscheint als Kontextkarte mit Name und Ergebnis des Befehls
ok | Keine Größengrenze | Die Ausgabe geht in ein rollierendes Protokoll, statt den Befehl bei 8 MiB abzubrechen; die KI erhält die neuesten Zeilen und kann bei Bedarf ältere lesen
ok | Aktivitätsbereich | Der Sitzungsbereich zählt Subagenten und Hintergrundbefehle und öffnet eine Liste mit ihren Protokollen; ein Neustart für ein Update warnt vor laufenden Jobs
ok | Immer da | Die Zähler erscheinen in jeder Sitzung, auch ohne Git, Subagenten oder laufende Befehle
```

### Projekte

```ui-checklist
ok | Projektstartseite | Der Projektname in der Seitenleiste öffnet eine Projektseite mit Umbenennen, Schnellaktionen, Sitzungsstatistik, einer durchsuchbaren Sitzungsliste und allen Projekteinstellungen
ok | Ungespeicherte Änderungen | Ungespeicherte Änderungen am Projektgedächtnis bleiben erhalten, wenn man die Projektseite verlässt und zurückkehrt
```

### Tray

```ui-checklist
ok | Doppelklick | Unter Windows öffnet ein Doppelklick auf das Tray-Symbol den gewählten Client: die Desktop-App, die WebUI oder den Terminal-Client; das Menü liegt jetzt auf der rechten Maustaste
ok | Sprache | Ein Sprachmenü im Tray wechselt die Oberflächensprache, und geöffnete Clients folgen ihr
```

### Oberfläche

```ui-checklist
ok | Lesebreite | Alle Breitenvorgaben für Unterhaltungen sind breiter, und eine breitere Spalte klappt die Navigation in gewöhnlichen Fenstern nicht mehr ein
ok | Statushinweis | Der Statushinweis sitzt jetzt am oberen Rand des Eingabefelds, statt über der Unterhaltung zu schweben
ok | Dunkle Designs | Markierter Text ist in dunklen Designs sichtbar
ok | Antwortkomponenten | Lange Schlüssel drücken die Werte nicht mehr in eine schmale Spalte, und Formeln in mehreren Gruppen vor einer Legende werden richtig angezeigt
ok | Werkzeugläufe | Laufende Werkzeugkarten in einem eingeklappten Lauf bleiben beieinander
```

### Fehlerbehebungen

```ui-checklist
ok | Planmodus | Nur ein eingereichter Plan wartet auf Bestätigung und wird nach einem Neustart wiederhergestellt
ok | Leere Antworten | Eine Antwort, die ohne Inhalt endet, wird nach Ihren Wiederholungseinstellungen erneut versucht; wird sie vor jeder Antwort durch das Ausgabelimit abgeschnitten, wird das angezeigt
ok | Sitzung fortsetzen | Eine Nachricht nach dem Aufräumen des Kontexts startet wieder zuverlässig
ok | Modellparameter | Ein Parameter für bestimmte Modelle wird nicht an andere Modelle gesendet
ok | MCP | Werkzeuglisten über mehrere Seiten werden vollständig gelesen, und selbst gesetzte Header haben Vorrang vor Headern aus Umgebungsvariablen
ok | Windows-Installer | Schlägt der Wechsel zur neuen Version fehl, nennt der Installer den Grund und behält die Dateien
ok | Beschädigte Installation | Eine Desktop-App mit unvollständiger Installation meldet das, statt den falschen Dienst zu starten
ok | Prüfungskorrekturen | Mehrere hundert kleinere Korrekturen in Sitzungen, Speicher, Gedächtnis, Dateien, Berechtigungen, Plugins, WebUI und Terminal-Client
```

### Sicherheit

```ui-checklist
ok | Eingebauter Browser | Der eingebaute Browser sperrt jede lokale Adressform des geschützten Core-Ports
ok | Anbieterfehler | API-Schlüssel werden aus Anbieterfehlern entfernt, bevor diese gekürzt oder angezeigt werden
ok | Plugin-Werkzeuge | Argumente, die nicht zu den Parametern eines Plugin-Werkzeugs passen, werden vor dessen Ausführung abgelehnt
```

### Terminal-Client

```ui-checklist
ok | Aktivität | Ein Aktivitäts-Tab, ein Zähler in der Sitzungsleiste und eine Protokollansicht für Hintergrundbefehle, dazu eine Warnung vor dem Neustart
ok | Korrekturen | Auswahllisten, Einfügen in Antworten auf Rückfragen, Mausauswahl, lange Menüs und Codeblöcke verhalten sich richtig
```

### Setup-Programm

```ui-checklist
ok | Wiederholung nach Fehler | Ein erneuter Setup-Lauf nach einem fehlgeschlagenen Versionswechsel registriert den Dienst wieder
ok | Mehrere Installationen | Die Deinstallation einer Installation lässt Starter, Menüeinträge und Dienste einer anderen Installation unberührt
```
