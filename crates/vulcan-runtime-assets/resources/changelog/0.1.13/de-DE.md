Vulcan Code 0.1.13 behebt den integrierten Browser auf Intel-Macs, Benachrichtigungen, die sich nicht mehr von selbst schlossen, und Laufzeitinstallationen, die dieselben Dateien erneut herunterluden.

### Aktualisierung auf diese Version

```ui-checklist
ok | Inkrementelles Update | Beim Update von 0.1.8 bis 0.1.12 werden nur die geänderten Dateien heruntergeladen
```

### Oberfläche

```ui-checklist
ok | Benachrichtigungen schließen sich wieder selbst | Nachdem Sie eine Benachrichtigung über ihre eigene Schließen- oder Aktionsschaltfläche geschlossen hatten, blieben alle folgenden Benachrichtigungen stehen, bis Sie sie von Hand schlossen; jetzt verschwinden sie wieder von selbst
ok | Integrierter Browser auf Intel-Macs | Der macOS-Intel-Build (x64) von 0.1.12 enthielt den Browser für Apple Silicon, der auf Intel-Macs nicht läuft; diese Version liefert den Intel-Browser aus, und jeder macOS-Build wird nun geprüft, ob alle enthaltenen Programme zu seinem Prozessor passen
```

### Laufzeitumgebungen

```ui-checklist
ok | Heruntergeladene Laufzeiten werden wiederverwendet | Beim Installieren oder Neuinstallieren von uv, Node.js oder pnpm wird ein zuvor heruntergeladenes Archiv wiederverwendet, solange es seine Prüfsumme noch besteht, statt es erneut herunterzuladen
```
