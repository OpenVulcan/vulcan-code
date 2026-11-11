O Vulcan Code 0.1.20 adiciona a transcrição na nuvem do OpenVulcan e pastas vinculadas ao projeto, além de melhorar a recuperação de turnos na fila e o diagnóstico de erros de provedores.

### Entrada de voz e área de trabalho

```ui-checklist
ok | Transcrição na nuvem | Entre no OpenVulcan em um site que ofereça o serviço. Nas configurações do microfone, escolha o modelo recomendado pelo site, um modelo específico da nuvem ou o modelo de reconhecimento de fala de um provedor configurado.
ok | Atalho de gravação | Defina um atalho para a entrada de voz e escolha o idioma de reconhecimento e as opções compatíveis com o modelo.
ok | Pastas vinculadas ao projeto | Adicione pastas locais na página inicial do projeto para navegá-las no painel de arquivos e abrir seus arquivos nas conversas. Repositórios Git vinculados aparecem na lista de alterações do projeto.
```

### Conversas e memória

```ui-checklist
ok | Recuperação de turnos na fila | Os turnos salvos são retomados com o identificador e as configurações do envio original. As metas em conversas existentes também são iniciadas uma única vez.
ok | Ramificações de conversa | Crie uma ramificação a partir de um turno concluído enquanto a conversa continua em execução. Os cartões de ferramentas mantidos na conversa exibem o estado final ao término da execução.
ok | Reconstrução de vetores de memória | Os lotes respeitam o limite de entradas por solicitação do modelo de embeddings. A prévia exibe o número de lotes usado na reconstrução.
```

### Modelos e diagnóstico de erros

```ui-checklist
ok | Descoberta de modelos do provedor | Consulte os modelos de conversa na conta selecionada de um provedor integrado e preencha o formulário de novo modelo com a sua escolha.
ok | Modelos da Alibaba Cloud | O Token Plan inclui Qwen4.0 Max Preview. As APIs do Model Studio em Pequim e Singapura incluem os modelos Qwen3.8.
ok | Detalhes de erros e novas tentativas | Os cartões mostram um diagnóstico curto e permitem copiar o JSON completo ou adicionar o erro atual à lista de novas tentativas. As regras podem ser editadas individualmente com palavras-chave ou expressões regulares.
```
