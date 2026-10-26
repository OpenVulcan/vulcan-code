Vulcan Code 0.1.0 é a primeira prévia pública: uma bancada gráfica de programação com IA que reúne projetos, conversas, terminais, navegador, revisão de código e commits do Git em um só lugar.

```ui-kv
Versão | Prévia
Clientes | Aplicativo para desktop, WebUI no navegador, cliente de terminal (TUI)
Idiomas da interface | 11
Licença | MIT
```

### Bancada de trabalho

```ui-cards
Projetos e conversas | Vários projetos e conversas lado a lado, com painéis de árvore de arquivos, revisão, memória e subagentes
Terminais do projeto | Terminais com várias abas; a IA pode manter sessões de comando longas ou interativas
Respostas ricas | Markdown, código, diagramas Mermaid e cartões de ferramentas em streaming; respostas interativas adicionam componentes visuais como achados, comparações, gráficos e fluxos
Ramificar e tentar de novo | Edite uma pergunta anterior para gerar de novo ou crie uma nova conversa a partir de qualquer turno concluído
Assistentes dedicados | Conversas com prompt base fixo e configurações congeladas para uma tarefa recorrente
```

### Fazendo o trabalho andar

```ui-cards
Modo objetivo | Defina um objetivo e um orçamento de Token; a IA continua trabalhando turno a turno e pode ser pausada, retomada ou limpa
Modo de planejamento | Primeiro explore e escreva um plano, depois execute quando você aprovar
Orientação em andamento | Adicione orientações ou enfileire novos pedidos enquanto uma tarefa roda
Subagentes | Papéis integrados de exploração e planejamento, além de modelos de papel opcionais, cada um limitado aos modelos que você permitir
Perguntas | Quando a IA precisa de uma decisão, ela pergunta com cartões de opções dentro da conversa
```

### Código, conhecimento e memória

```ui-cards
Navegação de código | O CodeKit lê a árvore de trabalho ao vivo: símbolos, definições e referências
Base de conhecimento | Bibliotecas que a IA mantém para você, com histórico permanente e acesso por projeto
Memória | Memória em Markdown na sua pasta de usuário ou memória de longo prazo do MemoryMesh, recuperada antes de cada turno
```

### Segurança e revisão

```ui-checklist
ok | Modos de permissão | Somente leitura, gravação no espaço de trabalho, aprovação automática e acesso total
ok | Fluxo de aprovação | Regras, um modelo de decisão e um modelo revisor; tudo que for incerto vai para você
ok | Revisão de Diff | Visualização unificada e dividida, mudanças por palavra, preparação e reversão
ok | Commits do Git | Mensagens escritas pela IA, commit no branch atual ou em um novo e depois push
ok | Segredos | Os valores das variáveis de ambiente nunca são mostrados à IA nem na interface
```

### Ampliando o Vulcan Code

```ui-cards
Habilidades | Habilidades de prompt (`SKILL.md`) e pacotes programáveis do LuaSkills
MCP | Conecte servidores MCP locais e HTTP ou deixe a IA conectar um apenas para uma conversa
Serviço MCP | Outras ferramentas de IA podem listar conversas, enviar mensagens e responder perguntas, cada uma com seu próprio Token
Plugins | Plugins adicionam ferramentas e páginas próprias, como o navegador de IA e as estatísticas de uso
Modelos | OpenAI, Anthropic, Gemini e outras famílias de protocolos, provedores integrados e personalizados, com modelos diferentes para cada finalidade
```

```ui-callout info Versão prévia
Os formatos de dados salvos ainda podem mudar entre as compilações 0.1.x. Atualizações que migram seus dados passo a passo começam na 0.2.
```
