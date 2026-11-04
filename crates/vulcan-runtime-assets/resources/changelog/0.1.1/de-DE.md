Vulcan Code 0.1.1 bringt Updates in der App, einen neuen Installationsweg und einen optionalen Benutzerdienst.

### Updates in der App

```ui-checklist
ok | Prüfen und herunterladen | Der Dienst sucht nach neuen Versionen, lädt nur geänderte Dateien und wechselt erst nach bestandener Signaturprüfung
ok | Neustart nach Bestätigung | Desktop-App, Browser-WebUI und TUI zeigen verfügbare Updates und starten nach Ihrer Bestätigung in der neuen Version neu
ok | Automatisches Zurücksetzen | Startet eine neue Version nicht, wechselt der Launcher zur vorherigen zurück und die App weist darauf hin
ok | Vollständige Updates | Ist eine vollständige Installation nötig, lädt die App das Installationsprogramm, führt es aus und öffnet sich danach wieder
```

### Installation und Dienst

```ui-cards
Setup-Programm | Das neue vulcan-code-setup installiert pro Benutzer unter Windows, macOS und Linux, wahlweise mit Desktop-App, Terminal-Client, Menüeinträgen und Desktop-Verknüpfung
Benutzerdienst | Der Dienst kann bei der Anmeldung starten: als Windows-Autostart-Eintrag, macOS-LaunchAgent oder systemd-Benutzerdienst unter Linux
Portbereich | Die Ports des Dienstes lassen sich in den erweiterten Einstellungen des Setup-Programms ändern
Terminal-Client | Läuft kein Dienst, startet die TUI wie die Desktop-App den installierten Dienst
```

```ui-callout info Upgrade von 0.1.0
0.1.0 hat noch keine Updates in der App. Führen Sie das Installationsprogramm oder den Installationsbefehl erneut aus, um auf 0.1.1 zu wechseln; spätere Versionen aktualisieren sich in der App.
```
