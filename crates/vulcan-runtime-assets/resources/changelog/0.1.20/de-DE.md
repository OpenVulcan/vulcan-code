Vulcan Code 0.1.20 ergänzt die Cloud-Transkription von OpenVulcan und zusätzliche Projektordner und verbessert die Wiederaufnahme eingereihter Gesprächsschritte sowie die Diagnose von Anbieterfehlern.

### Spracheingabe und Arbeitsbereich

```ui-checklist
ok | Cloud-Transkription | Melden Sie sich bei OpenVulcan auf einer Website an, die den Dienst anbietet. In den Mikrofoneinstellungen können Sie das von der Website empfohlene Modell, ein bestimmtes Cloud-Modell oder ein Spracherkennungsmodell eines eingerichteten Anbieters auswählen.
ok | Tastenkürzel für Aufnahmen | Legen Sie ein Tastenkürzel für die Spracheingabe fest und wählen Sie die Erkennungssprache und die vom Modell unterstützten Optionen.
ok | Zusätzliche Projektordner | Fügen Sie auf der Projektstartseite lokale Ordner hinzu. Sie können die Ordner im Dateibereich durchsuchen und ihre Dateien aus Gesprächen öffnen. Verknüpfte Git-Repositorys erscheinen in der Änderungsliste des Projekts.
```

### Gespräche und Gedächtnis

```ui-checklist
ok | Warteschlange wiederaufnehmen | Gespeicherte Gesprächsschritte werden mit ihrer ursprünglichen Sende-ID und den zugehörigen Einstellungen fortgesetzt. Ziele in bestehenden Gesprächen starten ebenfalls nur einmal.
ok | Gespräche verzweigen | Verzweigen Sie ein laufendes Gespräch ab einem abgeschlossenen Schritt. Die im Gespräch verbliebenen Werkzeugkarten zeigen nach der Ausführung ihren Endstatus.
ok | Gedächtnisvektoren neu aufbauen | Die Stapelgröße berücksichtigt die maximale Eingabezahl pro Anfrage des Embedding-Modells. Die Vorschau zeigt dieselbe Stapelanzahl wie die Ausführung.
```

### Modelle und Fehlerdiagnose

```ui-checklist
ok | Modelle eines Anbieters finden | Rufen Sie verfügbare Chatmodelle des ausgewählten Kontos eines integrierten Anbieters ab. Ihre Auswahl füllt das Formular für ein neues Modell vorab aus.
ok | Modelle von Alibaba Cloud | Der Token Plan enthält nun Qwen4.0 Max Preview. Die Model-Studio-Endpunkte in Peking und Singapur bieten Qwen3.8-Modelle an.
ok | Fehlerdetails und Wiederholungen | Fehlerkarten zeigen eine Kurzdiagnose. Sie können das vollständige JSON kopieren oder den aktuellen Fehler zur Wiederholungsliste hinzufügen. Regeln lassen sich einzeln bearbeiten und unterstützen Schlüsselwörter sowie reguläre Ausdrücke.
```
