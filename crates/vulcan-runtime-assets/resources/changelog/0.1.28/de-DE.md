Vulcan Code 0.1.28 führt eigene Arbeitsbereiche und einen Aufrufverlauf für externe MCP-Clients ein. Verwaltete ROOT-Skills unterhalb der geforderten Version werden beim Start aktualisiert.

### Externes MCP

```ui-checklist
ok | Eigene Projekte und Sitzungen | Externe Clients können Arbeitsverzeichnisse registrieren und eigene Sitzungen anlegen. Datei-, CodeKit-, Shell- und Wissensdatenbankwerkzeuge nutzen den Arbeitsbereich der Sitzungs-ID.
ok | Hintergrundbefehle und Terminals | Mit den passenden Berechtigungen können Clients Befehle im Hintergrund ausführen, Aufträge prüfen oder beenden und Terminalsitzungen nutzen.
ok | Optionaler Aufrufverlauf | Aktiviere die Protokollierung pro Client, um Argumente, Ergebnisse und Status einzusehen. Der Verlauf lässt sich nach Client, Projekt oder Sitzung filtern.
```

### Upgrade

```ui-checklist
ok | Eigenständige MCP-Aufrufe benötigen eine Sitzungs-ID | Registriere ein Projekt mit `vc_mcp_project`, erstelle mit `vc_mcp_session` eine Sitzung und übergib deren `sid` an eigenständige Werkzeuge. Die Sitzung bestimmt das Arbeitsverzeichnis.
```

### Skills und Entwicklungsprüfungen

```ui-checklist
ok | Verwaltete ROOT-Skills beim Start aktualisieren | CodeKit, Vulcan Curl und Vulcan Work werden aus der konfigurierten Quelle aktualisiert, wenn ihre Version das Minimum unterschreitet. Scheitert das Update, bleibt die nutzbare Version erhalten und der nächste Start versucht es erneut.
ok | HTTP-Abnahmetest für VMM-Richtlinien | `make test-vmm-http` prüft die WebUI-Richtliniensteuerung zusammen mit einem lokalen Dienst über HTTP.
```
