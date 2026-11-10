Vulcan Code 0.1.18 behebt Fehler bei der Kontextkomprimierung und verbessert die Tastenkürzel zum Senden, die Browsersteuerung und die Kostenschätzung.

### Kontext und Unterhaltungen

```ui-checklist
ok | Kontextkomprimierung | Die manuelle Komprimierung verwendet das ausgewählte Modell. Fehler wie fehlendes Guthaben oder eine fehlgeschlagene Anmeldung beenden automatische Wiederholungen. Fordert das Zusammenfassungsmodell ein Werkzeug an, bleibt die Vorschau der richtigen Komprimierungsanfrage zugeordnet.
ok | Automatische Wiederholungen | Fehler aus mehreren Versuchen derselben Anfrage erscheinen gemeinsam in einer Karte. Die einzelnen Versuche lassen sich dort durchblättern.
ok | Abgelaufener Gedächtniskontext | In der Unterhaltung wird angezeigt, wenn zuvor angehängter Gedächtniskontext abgelaufen ist.
```

### Bedienung und Einstellungen

```ui-checklist
ok | Warteschlange und Einschieben | Während einer Aufgabe lässt sich festlegen, ob Enter eine Nachricht einreiht oder einschiebt. Strg+Enter führt die jeweils andere Aktion aus. In einer inaktiven Unterhaltung wird sofort gesendet.
ok | Standardfreigabe und Denkstufe | Der Freigabemodus für neue Unterhaltungen lässt sich voreinstellen. Beim Wechsel zurück zu einem bereits verwendeten Modell wird dessen gespeicherte Denkstufe wiederhergestellt.
ok | Lokalen Browser schließen | Beim Schließen des Browserbereichs werden die Browserseiten der aktuellen Unterhaltung sofort geschlossen und deren Ressourcen freigegeben.
```

### Anbieter und Nutzung

```ui-checklist
ok | Anbieterhinweise | NodeAI erscheint erst nach der Konfiguration in der Anbieterliste. Als instabil gekennzeichnete Anbieter zeigen einen Warnhinweis.
ok | Anfragen nach Modell filtern | Die Liste der letzten Anfragen in der Nutzungsstatistik lässt sich nach Modell filtern.
ok | Preisstaffeln und Anbieterpreise | Die Kostenschätzung kann modellabhängige Tokenpreisstaffeln verwenden. Für einzelne Anbieter lassen sich eigene Modellpreise festlegen.
```

### Versionshinweise

```ui-checklist
ok | Versionshinweise | Die Hinweise zu 0.1.14 bis 0.1.17 sind wieder in Gruppen gegliedert und dadurch leichter zu überblicken.
```
