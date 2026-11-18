O Vulcan Code 0.1.27 adiciona um inspetor de contexto das solicitações, facilita a chamada de habilidades e plugins sob demanda e melhora a coordenação de subagentes e as permissões dos clientes MCP.

### Conversas e área de trabalho

```ui-checklist
ok | Inspecionar o contexto da solicitação | Abra o contexto pelo menu da conversa para revisar as instruções, mensagens e definições de ferramentas da última solicitação. Antes do primeiro envio, o inspetor mostra uma prévia do contexto que seria montado.
ok | Continuar a tarefa principal | A conversa principal pode processar mensagens prioritárias e resultados enquanto os subagentes ainda estão trabalhando. Os resultados entram nas solicitações seguintes.
ok | Posição de leitura e tempo decorrido | As atualizações em tempo real preservam a posição ao ler mensagens antigas. Ao reabrir a conversa, os eventos do Core restauram o tempo decorrido de uma solicitação ativa.
```

### Habilidades, plugins e MCP

```ui-checklist
ok | Mencionar habilidades e plugins no campo de entrada | Digite `@` para selecionar uma habilidade ou um caminho do projeto e `#` para selecionar um plugin. As sugestões exibem nomes curtos.
ok | Carregar ferramentas sob demanda | É possível chamar Vulcan Browser, Context7 e o gerenciamento de conexões MCP da conversa sob demanda. O LuaSkills 0.9.1 permite ativar esse modo para ferramentas de pacotes que incluem um guia.
ok | Conceder permissões MCP por grupo | Configure por cliente as permissões de conversa, ferramentas independentes e recursos multimodais. Autorize individualmente ferramentas de arquivos, shell, CodeKit e bases de conhecimento e delimite o acesso por projeto ou biblioteca.
```

### Instalação, modelos e feedback

```ui-checklist
ok | Instalador independente 0.2.18 | Na primeira instalação, o programa mede os tempos de resposta e recomenda a rota de download mais rápida. Você pode escolher outra. O LuaSkills usa o Hub e a fonte de download do GitHub da região selecionada.
ok | Diagnóstico de inicialização do LuaSkills | A tela inicial exibe o progresso da preparação das dependências no idioma selecionado e identifica quais falharam na instalação.
ok | Qwen4 Max | A versão de disponibilidade geral do Qwen4 Max agora aparece no Alibaba Cloud Token Plan.
ok | Contato opcional no feedback | Você pode incluir e conferir seus dados de contato antes de enviar feedback. As respostas públicas ficam na central de mensagens do aplicativo.
```
