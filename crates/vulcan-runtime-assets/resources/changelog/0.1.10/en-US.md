Vulcan Code 0.1.10 fixes editing and forking in long sessions, keeps turns going through rate limits and malformed tool arguments, and improves command cards and answer components.

### Updating to this version

```ui-checklist
ok | Incremental update | Updating from 0.1.8 or 0.1.9 downloads only the files that changed; earlier versions still install as a full update
ok | Setup installations | Installations made with the setup program update by running vulcan-code-setup update; the app says so instead of offering a download
```

### Sessions

```ui-checklist
ok | Editing and forking | In long sessions whose history spans several segments, editing a sent message or forking from a turn no longer fails with a provider_source fingerprint mismatch error
ok | Malformed tool arguments | A tool call whose arguments cannot be parsed gets an error result and the turn goes on, instead of ending at the second one; arguments that were cut short are reported as likely too long
ok | Rate limits | HTTP 429 rate limits are retried on the rate-limit schedule when "Wait and retry when rate-limited" is on instead of ending the turn; an exhausted balance still stops
```

### Tools and answers

```ui-checklist
ok | Command titles | The description argument of shell_command is now a short title of at most 80 characters, which keeps some models, such as MiniMax-M3.1-Flash, from breaking the tool call at that argument
ok | Background job cards | shell_job cards show what the command is for after the job id, so a wait shows which command it waits for; the terminal client titles command and job rows the same way
ok | Answer components | A third cell in a ui-kv row shows as a note under the value, and a signed change such as +5 in ui-metrics shows as a change, instead of the whole component falling back to code
```

### Models and memory

```ui-checklist
ok | OpenCode Go DeepSeek | deepseek-v4-pro and deepseek-v4.1-flash can turn thinking off, so they can be chosen as memory models
ok | Memory model note | The memory model settings now explain why only models that can turn thinking off are listed: the memory model is called before and after every reply, so thinking would add its wait and usage each time
```
