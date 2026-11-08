O Vulcan Code 0.1.13 corrige o navegador integrado em Macs com Intel, as notificações que deixavam de fechar sozinhas e as instalações de ambientes de execução que baixavam os mesmos arquivos de novo.

### Atualizando para esta versão

```ui-checklist
ok | Atualização incremental | Ao atualizar a partir das versões 0.1.8 a 0.1.12, apenas os arquivos alterados são baixados
```

### Interface

```ui-checklist
ok | Notificações voltam a fechar sozinhas | Depois de fechar uma notificação pelo próprio botão de fechar ou de ação, todas as notificações seguintes ficavam na tela até serem fechadas manualmente; agora voltam a desaparecer sozinhas
ok | Navegador integrado em Macs com Intel | A versão macOS Intel (x64) do 0.1.12 trazia o navegador para Apple silicon, que não roda em Macs com Intel; esta versão traz o navegador para Intel, e cada versão para macOS agora é verificada para que todos os seus programas correspondam ao processador
```

### Ambientes de execução

```ui-checklist
ok | Downloads reaproveitados | Ao instalar ou reinstalar uv, Node.js ou pnpm, um arquivo baixado antes é reaproveitado enquanto ainda passar na soma de verificação, em vez de ser baixado de novo
```
