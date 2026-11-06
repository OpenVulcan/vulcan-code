Vulcan Code 0.1.6 behebt den Neustart nach einem Update und das erneute Öffnen der App unter Linux, lässt ein Desktop-Fenster mit fehlgeschlagenem Start wieder schließen und bringt Setup 0.2.8 mit, das die ganze Installation im interaktiven Installer durchführt.

### Desktop

```ui-checklist
ok | Fehlgeschlagener Start | Schlägt der Start oder das Laden der Oberfläche fehl, zeigt das Fenster unter Windows und Linux Schaltflächen zum Neuladen und Schließen
ok | Belegtes Datenverzeichnis | Hält ein anderer Prozess das Datenverzeichnis und antwortet nicht, rät die Startseite, die vulcan-code-service-Prozesse zu beenden und die App erneut zu öffnen
```

### Linux

```ui-checklist
ok | Neustart nach Update | Nach einem Update startet der Core sofort neu; die Desktop-App öffnet die neue Version, ohne zwei Minuten zu warten
ok | Erneutes Öffnen | Wird die App geöffnet, während sie sich schließt, startet sie erneut, statt nichts zu tun; Hilfsprozesse der Desktop-App bleiben nicht mehr zurück
ok | Dock-Symbol | Der Menüeintrag von Setup 0.2.8 passt zum Fenster der Desktop-App, sodass ein im Dock angeheftetes Symbol zur App gehört; ein einmaliges Update mit dem Setup-Programm schreibt einen vorhandenen Eintrag neu
```

### Korrekturen

```ui-checklist
ok | Anbieterfehler | Meldet ein Anbieter mitten in einer Antwort einen Fehler, wird dessen eigene Meldung angezeigt statt eines Hinweises auf ein noch offenes Antwortelement (OpenAI-Chat- und Anthropic-Protokoll)
```

### Setup-Programm

```ui-checklist
ok | Interaktiver Installer | Nach dem Assistenten bleiben Übersicht, Download-Fortschritt und Ergebnis im interaktiven Installer, in der Sprache der Oberfläche; Updates nutzen ihn ebenfalls
ok | Genaue Downloadgröße | Die Übersicht zeigt genau, wie viel heruntergeladen wird; bereits vorhandene Dateien werden nicht erneut gezählt
ok | Nächstes Mal | Die letzte Seite erklärt, wie sich die Installation später ändern, aktualisieren oder deinstallieren lässt und dass sich das Setup-Programm zuerst selbst aktualisiert
ok | Abbrechen | Zurück oder Abbrechen in der Übersicht ändert nichts und endet ohne Fehlermeldung
ok | Installationsskripte | Die Installationsskripte zeigen Downloadquelle und Fortschritt und wechseln nach 30 Sekunden ohne Daten zu einem anderen Server
```
