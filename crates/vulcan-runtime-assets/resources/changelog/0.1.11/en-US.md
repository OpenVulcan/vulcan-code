Vulcan Code 0.1.11 adds feedback, a message center and a way to fix problems on your own device, and updates models for OpenAI, TokenRhythm, OpenCode Go and OpenDesign.

### Updating to this version

```ui-checklist
ok | Incremental update | Updating from 0.1.8, 0.1.9 or 0.1.10 downloads only the files that changed; earlier versions still install as a full update
ok | App database | On first start the app database is backed up and then upgraded to the new structure automatically, with nothing for you to do
```

### Feedback and messages

```ui-checklist
ok | Send feedback | When signed in, use "Send feedback" to report a problem or make a suggestion; before sending, secrets, paths, your user name, URLs and personal details are filtered out on your device and you see the exact content that will be sent
ok | Report from an error card | When a conversation fails, its error card offers a one-click report; it appears only for failures Vulcan Code may cause, not for sign-in, balance, rate limits, service outages, timeouts, content policy or cancellation
ok | Message center | The "Message center" shows announcements, personal messages and maintainer replies to your feedback; important announcements also show as a banner
ok | Fix it on this device | Choose a folder and a vulcan-code checkout is prepared as a project, with a conversation that has the fix skill; nothing is pushed or published until you confirm the pull request in that conversation
ok | GitHub entries | The account menu adds "Suggest on GitHub (issue)" and "Open a pull request on GitHub"
ok | Terminal client | The terminal client has the feedback page too
```

### Models and providers

```ui-checklist
ok | OpenAI | Eight public API models declare strict json_schema structured output; GPT-5.6 Sol/Terra/Luna, GPT-6 Sol/Luna and GPT-6.1 Terra can turn reasoning off
ok | TokenRhythm | Adds DeepSeek V4.1 Flash 1017, LongCat 2.0, MiniMax M3.1 Flash and the testing models Egg-Alpha and Neohorse 1 9B, and removes minimax-m2.5 and mimo-v2.5-pro; structured output is declared, Messages models now send thinking effort and can turn thinking off, DeepSeek V4 Pro 0813 and GLM-5.2 move to Responses and Kimi K2.6 to Chat
ok | OpenCode Go | Adds Gemini 4 Argon, Hy4 and the free Beam, Iceman and Crouton models; ten models declare structured output
ok | OpenDesign | GPT-6 Luna now uses Responses and Claude Haiku 5.5 uses Messages; adds deepseek-v4.1-flash-eco, hy4-preview, minimax-m3.1-flash and 14 Plus/Pro/Max chat models; image generation with GPT Image 2, Nano Banana 2/2 Lite and Seedream 5.0/5.0 Pro is available on plans that include it; thinking levels list only what each vendor publishes
ok | Memory models | More models can turn thinking off and return structured output, so more of them can be chosen as memory models
```
