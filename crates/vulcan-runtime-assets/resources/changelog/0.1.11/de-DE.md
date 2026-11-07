Vulcan Code 0.1.11 bringt Feedback, eine Nachrichtenzentrale und eine Möglichkeit, Probleme auf dem eigenen Gerät zu beheben, und aktualisiert Modelle für OpenAI, TokenRhythm, OpenCode Go und OpenDesign.

### Aktualisierung auf diese Version

```ui-checklist
ok | Inkrementelles Update | Beim Update von 0.1.8, 0.1.9 oder 0.1.10 werden nur geänderte Dateien heruntergeladen; ältere Versionen werden weiterhin vollständig installiert
ok | App-Datenbank | Beim ersten Start wird die App-Datenbank gesichert und dann automatisch auf die neue Struktur aktualisiert; Sie müssen nichts tun
```

### Feedback und Nachrichten

```ui-checklist
ok | Feedback senden | Angemeldet können Sie über „Feedback senden“ ein Problem melden oder einen Vorschlag machen; vor dem Senden werden Geheimnisse, Pfade, Ihr Benutzername, URLs und persönliche Angaben auf dem Gerät herausgefiltert, und Sie sehen genau den Inhalt, der gesendet wird
ok | Meldung aus einer Fehlerkarte | Schlägt eine Unterhaltung fehl, bietet die Fehlerkarte eine Meldung mit einem Klick an; nur bei Fehlern, die Vulcan Code verursachen kann, nicht bei Anmeldung, Guthaben, Ratenbegrenzung, Dienstausfällen, Zeitüberschreitungen, Inhaltsrichtlinien oder Abbruch
ok | Nachrichtenzentrale | Die „Nachrichtenzentrale“ zeigt Ankündigungen, persönliche Nachrichten und Antworten der Maintainer auf Ihr Feedback; wichtige Ankündigungen erscheinen zusätzlich als Banner
ok | Auf diesem Gerät beheben | Nach Auswahl eines Ordners wird ein vulcan-code-Checkout als Projekt vorbereitet und eine Unterhaltung mit dem Reparatur-Skill geöffnet; nichts wird gepusht oder veröffentlicht, bevor Sie den Pull-Request in dieser Unterhaltung bestätigen
ok | GitHub-Einträge | Das Kontomenü enthält jetzt „Vorschlag auf GitHub (Issue)“ und „Pull-Request auf GitHub erstellen“
ok | Terminal-Client | Auch der Terminal-Client hat die Feedback-Seite
```

### Modelle und Anbieter

```ui-checklist
ok | OpenAI | Acht Modelle der öffentlichen API deklarieren strikte strukturierte Ausgabe per json_schema; GPT-5.6 Sol/Terra/Luna, GPT-6 Sol/Luna und GPT-6.1 Terra können das Reasoning abschalten
ok | TokenRhythm | Neu sind DeepSeek V4.1 Flash 1017, LongCat 2.0, MiniMax M3.1 Flash sowie die Testmodelle Egg-Alpha und Neohorse 1 9B; minimax-m2.5 und mimo-v2.5-pro entfallen; strukturierte Ausgabe ist deklariert, Messages-Modelle senden jetzt die Denkstärke und können das Denken abschalten, DeepSeek V4 Pro 0813 und GLM-5.2 wechseln zu Responses und Kimi K2.6 zu Chat
ok | OpenCode Go | Neu sind Gemini 4 Argon, Hy4 und die kostenlosen Modelle Beam, Iceman und Crouton; zehn Modelle deklarieren strukturierte Ausgabe
ok | OpenDesign | GPT-6 Luna nutzt jetzt Responses und Claude Haiku 5.5 Messages; neu sind deepseek-v4.1-flash-eco, hy4-preview, minimax-m3.1-flash und 14 Plus/Pro/Max-Chatmodelle; Bildgenerierung mit GPT Image 2, Nano Banana 2/2 Lite und Seedream 5.0/5.0 Pro steht in Tarifen zur Verfügung, die sie enthalten; Denkstufen zeigen nur, was der jeweilige Anbieter veröffentlicht
ok | Gedächtnismodelle | Mehr Modelle können das Denken abschalten und strukturiert antworten und lassen sich daher als Gedächtnismodelle wählen
```
