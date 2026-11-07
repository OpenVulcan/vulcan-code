Vulcan Code 0.1.10 behebt das Bearbeiten und Abzweigen in langen Sitzungen, lässt Durchgänge bei Ratenbegrenzungen und fehlerhaften Tool-Argumenten weiterlaufen und verbessert Befehlskarten und Antwortkomponenten.

### Aktualisierung auf diese Version

```ui-checklist
ok | Inkrementelles Update | Beim Update von 0.1.8 oder 0.1.9 werden nur geänderte Dateien heruntergeladen; ältere Versionen werden weiterhin vollständig installiert
ok | Installation per Setup | Mit dem Setup-Programm installierte Versionen werden mit vulcan-code-setup update aktualisiert; die App weist darauf hin, statt einen Download anzubieten
```

### Sitzungen

```ui-checklist
ok | Bearbeiten und Abzweigen | In langen Sitzungen, deren Verlauf mehrere Segmente umfasst, schlagen das Bearbeiten einer gesendeten Nachricht und das Abzweigen von einem Durchgang nicht mehr mit dem Fehler provider_source fingerprint mismatch fehl
ok | Fehlerhafte Tool-Argumente | Ein Tool-Aufruf mit nicht lesbaren Argumenten erhält ein Fehlerergebnis und der Durchgang läuft weiter, statt beim zweiten Mal zu enden; abgeschnittene Argumente werden als vermutlich zu lang gemeldet
ok | Ratenbegrenzungen | HTTP 429 wird bei aktivierter Einstellung „Bei Ratenbegrenzung warten und wiederholen“ nach dem Zeitplan für Ratenbegrenzungen wiederholt, statt den Durchgang zu beenden; ein aufgebrauchtes Guthaben stoppt weiterhin
```

### Tools und Antworten

```ui-checklist
ok | Befehlstitel | Das Beschreibungsargument von shell_command ist jetzt ein kurzer Titel mit höchstens 80 Zeichen; so beschädigen manche Modelle wie MiniMax-M3.1-Flash den Tool-Aufruf an dieser Stelle nicht mehr
ok | Karten für Hintergrundaufträge | shell_job-Karten zeigen nach der Auftrags-ID den Zweck des Befehls, sodass beim Warten erkennbar ist, auf welchen Befehl gewartet wird; der Terminal-Client betitelt Befehls- und Auftragszeilen ebenso
ok | Antwortkomponenten | Eine dritte Zelle in einer ui-kv-Zeile erscheint als Notiz unter dem Wert, und eine vorzeichenbehaftete Änderung wie +5 in ui-metrics als Änderung, statt dass die ganze Komponente als Code angezeigt wird
```

### Modelle und Gedächtnis

```ui-checklist
ok | OpenCode Go DeepSeek | deepseek-v4-pro und deepseek-v4.1-flash können das Denken abschalten und daher als Gedächtnismodelle gewählt werden
ok | Hinweis zum Gedächtnismodell | Die Einstellungen erklären jetzt, warum nur Modelle mit abschaltbarem Denken aufgeführt sind: Das Gedächtnismodell wird vor und nach jeder Antwort aufgerufen, Denken würde jedes Mal Wartezeit und Verbrauch erhöhen
```
