### Korrekturen bei der Kontextkomprimierung

- Die Zusammenfassungsanfrage behält den vollständigen Gesprächsverlauf, die Werkzeugergebnisse und native Medien. Überschreitet die vollständige Eingabe das Kontextlimit des Modells, meldet die Komprimierung den Fehler und bewahrt den Verlauf.
- Geeignete offene Durchläufe lassen sich manuell komprimieren, solange keine Anfrage aktiv ist. Die ursprüngliche Aufgabe und vorgemerkte Eingaben bleiben erhalten. Die Aufgabe kann anschließend fortgesetzt werden.

### Weitere Verbesserungen

- Subagenten können eine vom Zielmodell unterstützte Denkstufe verwenden. Rückgaben von Subagenten, vorgemerkte Eingaben und die Anzeige ausstehender Nachrichten bleiben synchron.
- Die Nutzungsstatistik behält den gewählten Zeitraum und zeigt bei kurzen Zeiträumen stündliche Werte.
- Beim Zurückgehen in der Desktop-App bleibt der Startbildschirm geschlossen. Dateidialoge im Web passen sich der Fensterbreite an. Web- und Terminalclient zeigen Befunde mit `ok` als erfolgreiche Prüfungen an.
