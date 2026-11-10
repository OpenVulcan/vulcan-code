Vulcan Code 0.1.14 verbessert die macOS-Oberfläche und die Anzeige ungelesener Sitzungen und korrigiert die Kontextkomprimierung und Nutzungsstatistik.

### Desktop und Sitzungen

```ui-checklist
ok | macOS-Desktop | App-Paket, Dock und Menüleiste verwenden den Namen und das Symbol von Vulcan Code. Die nativen Fensterknöpfe befinden sich in der Titelleiste. Ein Doppelklick auf die Titelleiste schaltet den nativen Vollbildmodus um.
ok | Ungelesene Sitzungen | Sitzungen im Hintergrund erhalten nach Abschluss einen Markierungspunkt, der auch nach einem Neustart erhalten bleibt. Die ausgewählte Sitzung wird ebenfalls als ungelesen markiert, wenn sie bei ausgeblendetem oder inaktivem Fenster endet. Die Markierung bleibt bis zur nächsten Interaktion bestehen.
ok | Streamingstatus | Die Weboberfläche schließt verbliebene Denkprozess-Pop-ups am Ende einer Anfrage oder eines Durchlaufs und stellt nach der Wiederverbindung den Streamingstatus wieder her.
ok | Werkzeugaufrufe und Entscheidungskarten | Auch lange Aufrufsequenzen passen in die vorhandene Kopfzeile. Web- und Terminalclient erkennen empfohlene Optionen ohne Leerzeichen nach dem Sternchen sowie Entscheidungskarten, die direkt mit den Optionen beginnen.
```

### Kontext und Nutzung

```ui-checklist
ok | Kontextkomprimierung | Komprimierungsanfragen übernehmen die Werkzeugliste des ursprünglichen Durchlaufs, damit Anbieter mit Prompt-Caching das bereits gespeicherte Präfix wiederverwenden können.
ok | Nutzungsstatistik | Token-Summen und geschätzte Referenzkosten wurden korrigiert. Kostenschätzungen auf Basis unvollständiger Nutzungsdaten werden gekennzeichnet. Ändern sich die Statistiken beim Lesen, wiederholt das Dashboard die Abfrage einmal.
```
