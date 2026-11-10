O Vulcan Code 0.1.18 corrige erros na compactação de contexto e melhora os atalhos de envio, o fechamento do navegador e a estimativa de custos.

### Contexto e conversas

```ui-checklist
ok | Compactação de contexto | A compactação manual usa o modelo selecionado. Erros como saldo insuficiente ou falha de autenticação interrompem as novas tentativas automáticas. Se o modelo de resumo solicitar uma ferramenta, a prévia fica associada à solicitação de compactação correta.
ok | Histórico de tentativas | Os erros das novas tentativas automáticas de uma solicitação aparecem em um único cartão, onde é possível consultar cada tentativa.
ok | Avisos de expiração de memória | A conversa informa quando o contexto de memória anexado anteriormente expira.
```

### Controles e configurações

```ui-checklist
ok | Atalhos para enfileirar ou inserir | Durante uma tarefa, configure Enter para enfileirar ou inserir mensagens. Ctrl+Enter usa a outra ação. Quando a conversa está ociosa, a mensagem é enviada imediatamente.
ok | Aprovação padrão e raciocínio | Defina o modo de aprovação para novas conversas. Ao voltar a um modelo usado anteriormente, o nível de raciocínio salvo é restaurado.
ok | Fechar o navegador local | Fechar o painel do navegador na área de trabalho fecha imediatamente as páginas da conversa atual e libera seus recursos.
```

### Provedores e uso

```ui-checklist
ok | Avisos sobre provedores | O NodeAI aparece na lista depois de ser configurado. Provedores marcados como instáveis exibem um aviso.
ok | Filtrar solicitações por modelo | A lista de solicitações recentes nas estatísticas de uso pode ser filtrada por modelo.
ok | Preços por faixa e por provedor | As estimativas podem usar preços por faixa de tokens. Também é possível definir preços de modelo específicos para um provedor.
```

### Notas de versão

```ui-checklist
ok | Notas de versão | As notas da versão 0.1.14 à 0.1.17 voltaram a usar listas agrupadas para facilitar a leitura.
```
