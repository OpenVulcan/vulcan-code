### Correções na compactação de contexto

- A solicitação de resumo preserva todo o histórico da conversa, os resultados de ferramentas e a mídia nativa. Se a entrada completa exceder o limite de contexto do modelo, a compactação informa o erro e preserva o histórico.
- Turnos em aberto que atendem aos requisitos podem ser compactados manualmente quando nenhuma solicitação está ativa. A tarefa original e a entrada na fila são preservadas, permitindo continuar a tarefa depois.
- As mensagens na fila que você escolher inserir durante a compactação manual são enviadas na ordem dos cliques após a compactação ser concluída com sucesso. Se a compactação falhar, for interrompida ou o processo reiniciar, as mensagens permanecem na fila.

### Outras melhorias

- Os subagentes podem usar um nível de raciocínio compatível com o modelo de destino. Os resultados dos subagentes, a entrada na fila e o painel de mensagens pendentes ficam sincronizados.
- As estatísticas mantêm o período selecionado e mostram o uso por hora em períodos curtos.
- Voltar no aplicativo para desktop mantém a tela inicial de carregamento fechada. Os avisos de arquivos na web se ajustam à largura da janela, e os clientes web e de terminal exibem resultados `ok` como verificações bem-sucedidas.
