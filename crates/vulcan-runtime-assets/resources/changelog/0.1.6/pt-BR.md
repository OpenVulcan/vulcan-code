O Vulcan Code 0.1.6 corrige a reinicialização após uma atualização e a reabertura do aplicativo no Linux, permite fechar uma janela da área de trabalho cuja inicialização falhou e vem com o instalador 0.2.8, que faz toda a instalação no instalador interativo.

### Área de trabalho

```ui-checklist
ok | Falha na inicialização | Quando a inicialização ou o carregamento da interface falha, a janela mostra botões para recarregar e fechar no Windows e no Linux, para que possa ser fechada
ok | Diretório de dados ocupado | Quando outro processo ocupa o diretório de dados e não responde, a página de inicialização orienta a encerrar os processos vulcan-code-service e abrir o aplicativo novamente
```

### Linux

```ui-checklist
ok | Reinício após atualizar | Após uma atualização, o Core reinicia imediatamente; o aplicativo abre na nova versão sem esperar dois minutos
ok | Reabrir | Abrir o aplicativo enquanto ele está fechando o inicia de novo em vez de não fazer nada, e os processos auxiliares do aplicativo não ficam mais para trás
ok | Ícone do dock | A entrada de menu do instalador 0.2.8 corresponde à janela do aplicativo, então um ícone fixado no dock pertence a ele; executar uma vez a atualização do instalador reescreve uma entrada existente
```

### Correções

```ui-checklist
ok | Erros do provedor | Quando um provedor informa um erro no meio de uma resposta, a mensagem dele é exibida em vez de um aviso de que um item da resposta ainda estava aberto (protocolos OpenAI Chat e Anthropic)
```

### Instalador

```ui-checklist
ok | Instalador interativo | Depois do assistente, a revisão, o progresso do download e o resultado continuam no instalador interativo, no idioma da interface; as atualizações também o usam
ok | Download exato | A revisão mostra exatamente quanto será baixado; arquivos já presentes não são contados de novo
ok | Da próxima vez | A última página explica como alterar, atualizar ou desinstalar a instalação depois, e que o instalador se atualiza primeiro
ok | Cancelar | Voltar ou cancelar na revisão não altera nada e termina sem erro
ok | Scripts de instalação | Os scripts de instalação mostram a origem e o progresso do download e passam para outro servidor após 30 segundos sem dados
```
