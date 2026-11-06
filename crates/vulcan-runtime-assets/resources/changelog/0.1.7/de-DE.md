Mit Vulcan Code 0.1.7 startet die Linux-Desktop-App unter Ubuntu 23.10 und neuer aus dem Dock und dem App-Menü, Neuinstallationen und Updates unter Windows werden sicherer, mehrere Sicherheits- und Datenverlustlücken sind geschlossen, und das Setup-Programm 0.2.9 ist enthalten.

### Update auf diese Version

```ui-checklist
ok | Vollständiges Update | Diese Version ändert den Installer und das Installationslayout und wird daher als vollständiges Update installiert: Unter Windows lädt die App den Installer einmal herunter und führt ihn aus
ok | Setup-Installationen | Mit dem Setup-Programm erstellte Installationen werden mit vulcan-code-setup update aktualisiert; die App weist darauf hin, statt einen Download anzubieten
```

### Linux

```ui-checklist
ok | Dock und App-Menü | Unter Ubuntu 23.10 und neuer installiert Setup 0.2.9 einmalig ein AppArmor-Profil, sodass die Desktop-App aus dem Dock und dem App-Menü startet und nicht nur über ein Desktop-Symbol
ok | Profil aus der App | Fehlt das Profil oder ist es veraltet, bieten die Update-Einstellungen an, es mit einmaliger Eingabe des Administratorpassworts zu installieren, oder zeigen den Befehl für ein Terminal
```

### Windows

```ui-checklist
ok | Sichere Neuinstallation | Eine Installation in einen Ordner ohne frühere Installation entfernt dort nie Dateien; bricht die Neuinstallation derselben Version ab, wird die vorherige wiederhergestellt
ok | Probestart | Wird die Desktop-App geöffnet, während der vorherige Core noch läuft, zählt das nicht mehr als fehlgeschlagener Start der neuen Version und löst kein Zurücksetzen aus
```

### Sicherheit

```ui-checklist
ok | Lokaler Dienst | Der Core nimmt nur Anfragen an einen Loopback-Hostnamen an, sodass eine Webseite, die ihre eigene Adresse auf diesen Computer zeigen lässt, den Dienst nicht lesen kann
ok | Anhänge | SVG-Anhänge öffnen sich in einer Sandbox und können keine Skripte ausführen; Anhangspfade können ihren Ordner nicht verlassen
ok | Nur-Lese-Prüfung | Erweiterungswerkzeuge mit einem Schreibverb im Namen, die schreibenden Optionen von git branch und die Ausführungsoptionen von fd gelten nicht mehr als nur lesend
ok | Geheimnisse | Wird eine geheime Umgebungsvariable mit einem normalen Wert überschrieben, bleibt sie geheim
```

### Fehlerbehebungen

```ui-checklist
ok | Skills und Projekte | Ein lokaler Skill, der aus seinem eigenen Ordner installiert wird, löscht sich nicht mehr, und beim Löschen eines Projekts werden auch seine Projektvariablen entfernt
ok | Gedächtnis | Das Markdown-Gedächtnis liest chinesische, japanische und koreanische Überschriften richtig
ok | Planmodus | Das Ein- oder Ausschalten des Planmodus nach dem Ende einer Runde schlägt nicht mehr fehl
ok | Werkzeugparameter | Parameter, die auf gemeinsame Definitionen verweisen, etwa Listen erlaubter Werte, bleiben sichtbar und erforderlich
ok | Modellparameter | Dezimalparameter werden als Zahlen gesendet, und Tavily Extract funktioniert nur mit URLs
ok | Dateibearbeitung | Einfügen am Dateiende erhält die letzte Zeile und abschließende Leerzeilen
ok | WebUI | Pfeiltasten in der Commit-Nachricht bewegen den Cursor, Diff-Zeilen, die mit --- oder +++ beginnen, werden angezeigt, und Codebeispiele behalten ihre Bildlinks wie geschrieben
```

### Terminal-Client

```ui-checklist
ok | Folge-Prompts | Prompts an eine bestehende Sitzung werden angenommen, und ein nicht angenommener Prompt kehrt ins Eingabefeld zurück
ok | Git | Eine unterbrochene Verbindung sperrt Commit und Nachrichtenerzeugung nicht mehr bis zum Neustart
ok | Denkstufen | Jede Denkstufe wird wie gewählt gesendet
```

### Setup-Programm

```ui-checklist
ok | AppArmor-Profil | Setup 0.2.9 installiert das Profil über sudo, wenn das System es braucht, und hinterlässt den genauen Befehl in den Hinweisen, wenn das Passwort nicht eingegeben wird; die Deinstallation entfernt es
```
