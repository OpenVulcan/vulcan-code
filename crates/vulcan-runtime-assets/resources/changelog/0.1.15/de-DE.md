Vulcan Code 0.1.15 korrigiert die Kontextkomprimierung, damit die vollständige Eingabe erhalten bleibt und geeignete offene Durchläufe nach manueller Komprimierung fortgesetzt werden können.

### Korrekturen bei der Kontextkomprimierung

```ui-checklist
ok | Vollständige Eingabe bei der Komprimierung | Die Zusammenfassungsanfrage behält den vollständigen Gesprächsverlauf, die Werkzeugergebnisse und native Medien. Überschreitet die vollständige Eingabe das Kontextlimit des Modells, meldet die Komprimierung den Fehler und bewahrt den Verlauf.
ok | Offene Durchläufe manuell komprimieren | Geeignete offene Durchläufe lassen sich manuell komprimieren, solange keine Anfrage aktiv ist. Die ursprüngliche Aufgabe und vorgemerkte Eingaben bleiben erhalten. Die Aufgabe kann anschließend fortgesetzt werden.
ok | Vorgemerkte Nachrichten nach der Komprimierung | Während der manuellen Komprimierung ausgewählte Nachrichten werden nach erfolgreicher Komprimierung in der Reihenfolge der Klicks gesendet. Schlägt die Komprimierung fehl, wird sie gestoppt oder der Prozess neu gestartet, bleiben die Nachrichten in der Warteschlange.
```

### Weitere Verbesserungen

```ui-checklist
ok | Denkstufe von Subagenten und Warteschlange | Subagenten können eine vom Zielmodell unterstützte Denkstufe verwenden. Rückgaben von Subagenten, vorgemerkte Eingaben und die Anzeige ausstehender Nachrichten bleiben synchron.
ok | Stündliche Nutzungsstatistik | Die Nutzungsstatistik behält den gewählten Zeitraum und zeigt bei kurzen Zeiträumen stündliche Werte.
ok | Korrekturen an Desktop und Oberfläche | Beim Zurückgehen in der Desktop-App bleibt der Startbildschirm geschlossen. Dateidialoge im Web passen sich der Fensterbreite an. Web- und Terminalclient zeigen Befunde mit ok als erfolgreiche Prüfungen an.
```
