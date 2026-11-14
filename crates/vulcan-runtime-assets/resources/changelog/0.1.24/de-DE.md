Vulcan Code 0.1.24 verbessert Anhänge und die Wiederherstellung beim Modellwechsel und behebt Probleme im integrierten Browser sowie in Terminalprotokollen.

### Gespräche und Eingabe

```ui-checklist
ok | Dateien ziehen und einfügen | Die Anhangsvorschau ist wieder verfügbar. Dateien, die das Modell nicht hochladen kann, lassen sich bei bekanntem Originalpfad als Verweis senden. Gesendete Verweise lassen sich getrennt vom Nachrichtentext aufklappen.
ok | Modellwechsel und Wiederherstellung | Beim Modellwechsel bleibt der ursprüngliche Gesprächsverlauf erhalten. Lässt sich die bisherige Schlussfolgerung nicht sicher wiedergeben, können Sie sichtbare Nachrichten als Entwurf in ein neues Gespräch übernehmen und vor dem Senden prüfen.
ok | Kompaktierung und Scrollen | Die laufende Kontextkompaktierung wird separat angezeigt. Beim Verfolgen des Gesprächsendes bleibt neuer Text sichtbar.
```

### Integrierter Browser

```ui-checklist
ok | Darstellung und Bearbeitung | Das Laden von Stylesheets und Ressourcen von Pop-ups wurde korrigiert und die Bearbeitung verbessert. Passwörter bleiben im integrierten Browser verdeckt, wenn die Seite ihre Sichtbarkeit umschaltet.
```

### Protokolle und Anbieter

```ui-checklist
ok | Terminalprotokolle | Gespeicherte Geheimnisse über Zeilenumbrüche und abgeschnittene Ausgaben hinweg werden besser maskiert. Hintergrundaufgaben lassen sich auch anhalten, wenn die Maskierung nicht verfügbar ist.
ok | Anbieterfehler | Fehlerkarten behalten verständliche Anbieternachrichten und Hilfelinks bei und verdecken URLs mit Anmeldedaten.
ok | Anbieterkompatibilität | Alibaba Cloud Model Studio Token Plan versucht keine temporären OSS-Uploads mehr. Anfragen von Wissensdatenbanken mit OpenVulcan-Embeddings wurden korrigiert.
```
