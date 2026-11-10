Vulcan Code 0.1.15 corrige a compactação de contexto para preservar toda a entrada e permite retomar turnos elegíveis após a compactação manual.

### Correções na compactação de contexto

```ui-checklist
ok | Entrada completa na compactação | A solicitação de resumo preserva todo o histórico da conversa, os resultados de ferramentas e a mídia nativa. Se a entrada completa exceder o limite de contexto do modelo, a compactação informa o erro e preserva o histórico.
ok | Compactação manual de turnos em aberto | Turnos em aberto que atendem aos requisitos podem ser compactados manualmente quando nenhuma solicitação está ativa. A tarefa original e a entrada na fila são preservadas, permitindo continuar a tarefa depois.
ok | Mensagens na fila após a compactação | As mensagens na fila que você escolher inserir durante a compactação manual são enviadas na ordem dos cliques após a compactação ser concluída com sucesso. Se a compactação falhar, for interrompida ou o processo reiniciar, as mensagens permanecem na fila.
```

### Outras melhorias

```ui-checklist
ok | Raciocínio dos subagentes e fila de mensagens | Os subagentes podem usar um nível de raciocínio compatível com o modelo de destino. Os resultados dos subagentes, a entrada na fila e o painel de mensagens pendentes ficam sincronizados.
ok | Uso por hora | As estatísticas mantêm o período selecionado e mostram o uso por hora em períodos curtos.
ok | Correções no aplicativo e na interface | Voltar no aplicativo para desktop mantém a tela inicial de carregamento fechada. Os avisos de arquivos na web se ajustam à largura da janela, e os clientes web e de terminal exibem resultados ok como verificações bem-sucedidas.
```
