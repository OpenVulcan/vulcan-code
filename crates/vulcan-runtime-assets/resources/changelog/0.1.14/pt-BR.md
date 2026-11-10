Vulcan Code 0.1.14 melhora o aplicativo para macOS e os indicadores de sessões não lidas, e corrige a compactação de contexto e as estatísticas de uso.

### Área de trabalho e sessões

```ui-checklist
ok | Aplicativo para macOS | O pacote do aplicativo, o Dock e a barra de menus usam o nome e o ícone do Vulcan Code. Os controles nativos ficam na barra de título, onde um clique duplo alterna a tela cheia nativa.
ok | Sessões não lidas | As sessões em segundo plano recebem um indicador de não lida ao terminar e o mantêm após reiniciar. A sessão selecionada também recebe o indicador se terminar com a janela oculta ou sem foco, até que você interaja com ela novamente.
ok | Estado da transmissão | A interface web fecha os pop-ups de raciocínio que permaneciam abertos após o fim de uma solicitação ou de um turno e restaura o estado da transmissão após reconectar.
ok | Chamadas de ferramentas e cartões de decisão | Sequências longas cabem no cabeçalho existente. Os clientes web e de terminal reconhecem opções recomendadas sem espaço após o asterisco e cartões que começam diretamente pelas opções.
```

### Contexto e uso

```ui-checklist
ok | Compactação de contexto | As solicitações de compactação preservam a lista de ferramentas do turno de origem para que os provedores com cache de prompts possam reutilizar o prefixo armazenado.
ok | Estatísticas de uso | Os totais de tokens e as estimativas de custo de referência foram corrigidos. As estimativas baseadas em dados parciais são sinalizadas. Se as estatísticas mudarem durante a leitura, o painel consulta os dados novamente uma vez.
```
