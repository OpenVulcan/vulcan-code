Vulcan Code 0.1.5 gibt jedem Modell ein eigenes Standard-Komprimierungsfenster, startet die Desktop-App in einem einzigen Fenster und lässt Sie festlegen, wie oft nach Updates gesucht wird.

### Kontextfenster

```ui-checklist
ok | Standard pro Modell | Jedes aktivierte Gesprächsmodell kann ein eigenes Standard-Komprimierungsfenster für neue Sitzungen haben: Systemstandard, dem Modell folgen, eine feste Stufe oder eine eigene Größe; bestehende Sitzungen bleiben unverändert
ok | Eigene Fenster | Eine eigene Größe nimmt eine Token-Zahl oder K für Tausend an (etwa 235K), mindestens 100K und höchstens das Modellfenster; auch die Fensterauswahl neben dem Eingabefeld kann auf eine eigene Größe wechseln
ok | Modell wechseln | Beim Modellwechsel in einer Sitzung wird das Standardfenster des Zielmodells angeboten, und nichts ändert sich, bevor Sie bestätigen; ein Fenster, das das Zielmodell nicht fassen kann, folgt nun dem Modell, statt den Wechsel zu verhindern
ok | Terminaloberfläche | Auch in den TUI-Einstellungen lässt sich das Standardfenster pro Modell festlegen
```

### Updates

```ui-checklist
ok | Prüfintervall | Legen Sie fest, wie oft automatisch nach Updates gesucht wird: stündlich, alle 6 Stunden, alle 12 Stunden oder täglich, standardmäßig alle 6 Stunden; eine Änderung gilt ohne Neustart
ok | Seite Software-Update | Die Seite Software-Update in den Einstellungen ist neu geordnet: zuerst die aktuelle Version mit Prüfen und Reparieren, dann die Update-Einstellungen und zuletzt das gefundene Update mit den Notizen aller enthaltenen Versionen, die neueste zuerst; der Update-Dialog zeigt dieselbe Versionsliste
ok | Neustart unter Linux | Unter Linux startet ein von der Desktop-App gestarteter Core nach einem Update neu, statt bei „Wird neu gestartet“ hängen zu bleiben
```

### Desktop-App

```ui-checklist
ok | Start in einem Fenster | Es öffnet sich kein eigenes Startfenster mehr; das Arbeitsfenster zeigt den Startbildschirm mit den tatsächlichen Startschritten und etwaigen Fehlern und öffnet die Oberfläche an Ort und Stelle, sobald Core bereit ist
ok | Aussehen beim Start | Der Startbildschirm übernimmt Sprache und Erscheinungsbild vom letzten Mal; der erste Start folgt dem System
ok | App-Symbol | Das Linux-Dock und das macOS-Dock zeigen das Vulcan-Code-Symbol statt eines allgemeinen Symbols; erneutes Öffnen der App holt das startende Fenster nach vorn
```

### Fehlerbehebungen

```ui-checklist
ok | Embedding-Modelle | Embedding-Modelle mit einstellbarer Ausgabegröße (etwa Qwen3-Embedding-8B, 32 bis 4096 Dimensionen) weisen nicht mehr jede Antwort zurück
ok | Antwortkomponenten | Hinweisblöcke ohne Ton und Befundzeilen mit vier Zellen werden jetzt dargestellt, statt auf Codeblöcke zurückzufallen, in WebUI und TUI
```

### Installationsprogramm

```ui-checklist
ok | Einzeilige Installation unter macOS | Das Installationsskript kann unter macOS über eine curl-Pipe jetzt die Tastatur lesen
ok | macOS-App | Die vom Installationsprogramm erstellte macOS-App hat ein Symbol, und am Ende wird angezeigt, wo die App liegt
ok | Linux-Menüeintrag | Der Linux-Menüeintrag passt zum laufenden Fenster, sodass ein im Dock angeheftetes Symbol die App öffnet
```
