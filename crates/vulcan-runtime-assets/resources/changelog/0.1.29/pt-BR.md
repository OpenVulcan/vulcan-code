O Vulcan Code 0.1.29 mantém a conexão em chamadas longas a ferramentas MCP externas e atualiza o catálogo de modelos da OpenAI.

### MCP externo

```ui-checklist
ok | Sinais de atividade em chamadas longas | Quando o cliente aceita SSE, uma chamada longa envia sinais periódicos para reduzir desconexões por inatividade. A mesma solicitação recebe o resultado final quando a ferramenta termina.
ok | Comandos longos em segundo plano | Os prazos de espera do cliente continuam valendo. Se uma solicitação de Shell em primeiro plano perder a conexão, o serviço encerra sua árvore de processos. Inicie comandos longos com `background: true` e consulte o resultado com `shell_job` usando o mesmo `sid`. É necessária a permissão `shell_job`.
ok | Diagnóstico de tokens mais claro | Os registros do serviço distinguem cabeçalhos de autenticação ausentes ou malformados de tokens inválidos e registram as trocas manuais. As entradas contêm apenas a categoria do diagnóstico ou o ID do cliente.
```

### Modelos

```ui-checklist
ok | GPT-6.2 Sol e Astra | Os dois modelos estão disponíveis na API da OpenAI e nas assinaturas do Codex. As assinaturas do Codex usam a janela de contexto informada pelo catálogo de modelos.
ok | GPT Image 2.6 | Flare e Sunburst agora estão disponíveis no seletor de mídia da OpenAI.
```
