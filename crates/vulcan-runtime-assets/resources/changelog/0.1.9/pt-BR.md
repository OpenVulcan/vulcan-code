O Vulcan Code 0.1.9 corrige sessões longas que ficavam minutos em uma solicitação depois do envio de uma mensagem e não podiam ser interrompidas, e corrige dois problemas do aplicativo de desktop no Linux e no macOS.

### Atualizar para esta versão

```ui-checklist
ok | Atualização incremental | Ao atualizar a partir da 0.1.8, só os arquivos alterados são baixados; versões anteriores continuam sendo instaladas como atualização completa
ok | Instalações pelo instalador | Instalações feitas com o programa de instalação são atualizadas com vulcan-code-setup update; o aplicativo avisa isso em vez de oferecer um download
```

### Sessões

```ui-checklist
ok | Sem travamentos | Sessões longas cujo histórico ocupa vários segmentos, como as que têm muita saída de ferramentas, não esperam mais minutos depois do envio de uma mensagem, e o botão de parar age na hora
ok | Depois de reiniciar | Depois de reiniciar o aplicativo, as respostas anteriores dessas sessões continuam sendo enviadas ao modelo no formato próprio dele, com o raciocínio, em vez de texto simples
```

### Linux e macOS

```ui-checklist
ok | Mover a janela | No Linux, a barra de título volta a mover a janela logo depois que ela é restaurada do estado maximizado
ok | Navegador integrado | No Linux e no macOS, o navegador integrado instalado funciona
```
