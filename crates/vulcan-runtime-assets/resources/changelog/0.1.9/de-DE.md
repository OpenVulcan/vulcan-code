Vulcan Code 0.1.9 behebt lange Sitzungen, die nach dem Senden einer Nachricht minutenlang bei einer Anfrage hingen und sich nicht stoppen ließen, und behebt zwei Desktop-Probleme unter Linux und macOS.

### Auf diese Version aktualisieren

```ui-checklist
ok | Inkrementelles Update | Beim Update von 0.1.8 werden nur die geänderten Dateien geladen; ältere Versionen werden weiterhin als vollständiges Update installiert
ok | Installationen per Setup | Mit dem Setup-Programm erstellte Installationen werden mit vulcan-code-setup update aktualisiert; die App weist darauf hin, statt einen Download anzubieten
```

### Sitzungen

```ui-checklist
ok | Kein Hängen mehr | Lange Sitzungen, deren Verlauf mehrere Segmente umfasst, etwa Sitzungen mit viel Werkzeugausgabe, warten nach dem Senden einer Nachricht nicht mehr minutenlang, und die Stopp-Schaltfläche wirkt sofort
ok | Nach einem Neustart | Nach einem Neustart der App werden frühere Antworten solcher Sitzungen weiterhin im eigenen Format des Modells samt Denkinhalt zurückgesendet, statt als reiner Text
```

### Linux und macOS

```ui-checklist
ok | Fenster verschieben | Unter Linux lässt sich das Fenster direkt nach dem Wiederherstellen aus dem maximierten Zustand wieder über die Titelleiste verschieben
ok | Integrierter Browser | Unter Linux und macOS funktioniert der installierte integrierte Browser
```
