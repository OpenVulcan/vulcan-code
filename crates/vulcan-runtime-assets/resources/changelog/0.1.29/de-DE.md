Vulcan Code 0.1.29 sendet bei langen externen MCP-Toolaufrufen Heartbeats und aktualisiert den OpenAI-Modellkatalog.

### Externes MCP

```ui-checklist
ok | Heartbeats für lange Toolaufrufe | Unterstützt der Client SSE, sendet ein längerer Aufruf regelmäßig Heartbeats, damit die Verbindung seltener wegen Inaktivität abbricht. Nach Abschluss erhält dieselbe Anfrage das endgültige Ergebnis.
ok | Lange Befehle im Hintergrund ausführen | Die Zeitlimits des Clients gelten weiterhin. Wird eine Shell-Anfrage im Vordergrund getrennt, beendet der Dienst ihren Prozessbaum. Lange Befehle mit `background: true` starten und das Ergebnis mit `shell_job` und derselben `sid` abrufen. Dazu ist die Berechtigung `shell_job` erforderlich.
ok | Genauere Token-Diagnose | Das Dienstprotokoll unterscheidet fehlende oder fehlerhafte Authentifizierungsheader von ungültigen Tokens und erfasst manuelle Tokenwechsel. Es enthält nur die Diagnosekategorie oder die Client-ID.
```

### Modelle

```ui-checklist
ok | GPT-6.2 Sol und Astra | Beide Modelle sind über die OpenAI API und Codex-Abonnements verfügbar. Codex-Abonnements verwenden das im Modellkatalog gemeldete Kontextfenster.
ok | GPT Image 2.6 | Flare und Sunburst sind jetzt in der OpenAI-Medienauswahl verfügbar.
```
