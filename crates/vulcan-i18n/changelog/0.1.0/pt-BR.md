Vulcan Code 0.1.0 é a primeira prévia pública: uma bancada gráfica de IA para escrever código e para as tarefas de trabalho do dia a dia. Projetos, sessões, terminais, navegador, revisão de código e commits do Git ficam em um só lugar.

```ui-kv
Versão | Prévia
Clientes | Aplicativo para desktop, WebUI no navegador, cliente de terminal (TUI)
Idiomas da interface | 11
Pacotes | Python, Node.js e componentes do navegador incluídos; instalação com um comando
Licença | MIT
```

### Destaques

````ui-tabs
### Janela de compressão
Cada sessão pode trocar a janela de compressão a qualquer momento pelo indicador de capacidade de contexto na área de entrada, então uma tarefa longa nunca precisa de uma nova sessão.

```ui-kv
Tamanhos | Cinco níveis, por padrão 200K, 256K, 272K, 373K e 500K tokens, editáveis nas configurações
Seguir o modelo | Escolha "Padrão (segue o modelo)" para comprimir pela janela do modelo atual
Alcance | Muda apenas quando a compressão automática acontece, não a janela de contexto do próprio modelo
Compressão manual | Execute /compact na sessão atual a qualquer momento
```

### Cartões de ferramentas e recolhimento
Cada tipo de ferramenta tem seu próprio cartão, então nem muitas chamadas de ferramentas escondem a resposta.

```ui-checklist
ok | Cartões dedicados | Arquivos, comandos, terminais, subagentes, pesquisa, mídia, perguntas, variáveis de ambiente, alterações de configuração e mais
ok | Tempo e links | Cada chamada mostra quanto tempo levou; arquivos em um cartão abrem direto no painel de arquivos
ok | Recolhimento sob demanda | Aberto durante a execução e recolhido ao terminar; chamadas consecutivas se agrupam sob um único resumo
ok | Troque quando quiser | Escolha o padrão nas configurações ou troque por sessão com "Recolhimento de ferramentas" na área de entrada ou /collapse
```

### Multimodal
Em "Extensões multimodais" cada capacidade recebe um ou vários modelos; "Capacidade estendida" as ativa por sessão.

```ui-cards
Web | Pesquisa na web e extração de páginas
Compreensão | Compreensão de imagem, vídeo e áudio
Geração | Geração de imagem e vídeo; as imagens também podem usar uma assinatura do Codex
Voz e música | Síntese e transcrição de voz, geração de música e covers
```

### Variáveis de ambiente
O gerenciamento integrado de variáveis de ambiente mantém os segredos na sua máquina; a IA só vê os nomes das variáveis.

```ui-checklist
ok | Três camadas | Camadas global, de projeto e de sessão editadas na interface; a página pode ser mostrada na barra lateral
ok | Valores ocultos | Os valores nunca são mostrados à IA nem na interface, e valores que aparecem em resultados de ferramentas são mascarados
ok | Preenchidas sob pedido | Quando falta uma variável, a IA pede o valor a você em um formulário, e ele nunca passa pela conversa
ok | Injetadas automaticamente | Comandos e terminais recebem as variáveis da sua camada; servidores MCP usam as globais
```

### Modelos de subagentes
Os subagentes só usam os modelos que você permitir.

```ui-kv
Dois níveis | A configuração do sistema é o padrão; cada sessão pode definir a sua
Marcado por padrão | O modelo chamador vem incluído por padrão e pode ser desmarcado
Fora do escopo | Recusado de imediato, nunca substituído silenciosamente por outro modelo
Configurações da sessão | Altere a qualquer momento em Configurações da sessão, abaixo da área de entrada, ou volte ao padrão do sistema com um clique
```
````

### Bancada de trabalho

```ui-cards
Projetos e sessões | Vários projetos e sessões lado a lado na ordem que você escolher, com painéis de árvore de arquivos, revisão, memória e subagentes
Terminais do projeto | Terminais com várias abas; a IA pode manter sessões de comando longas ou interativas
Painel de arquivos | Mencione arquivos com @, arraste-os para a área de entrada, abra caminhos a partir de terminais e da saída, pesquise e vá para uma linha
Respostas ricas | Markdown, código e diagramas Mermaid em streaming; respostas interativas adicionam checklists, comparações, gráficos, fluxos e mais, e cartões de decisão inserem ou enviam sua resposta
Ramificar e tentar de novo | Edite uma pergunta anterior para gerar de novo ou crie uma nova sessão a partir de qualquer turno concluído
Paleta de comandos | Atalhos de teclado, busca na conversa e cópia de uma conversa como Markdown
Assistentes dedicados | Sessões com prompt base fixo e configurações congeladas para uma tarefa recorrente
```

### Fazendo o trabalho andar

```ui-cards
Modo de objetivo | Defina um objetivo e um orçamento de tokens; a IA continua trabalhando turno a turno e pode ser pausada, retomada ou limpa
Modo de planejamento | Primeiro explore e escreva um plano, depois execute quando você aprovar
Orientação | Adicione orientações ou enfileire novos pedidos enquanto uma tarefa roda
Subagentes | Papéis integrados de exploração e planejamento, além de modelos de papel opcionais
Perguntas | Quando a IA precisa de uma decisão, ela pergunta com cartões de opções dentro da sessão
```

### Código, conhecimento e memória

```ui-cards
Navegação de código | O CodeKit lê a árvore de trabalho ao vivo: símbolos, definições e referências
Base de conhecimento | Bibliotecas que a IA mantém para você, com histórico permanente e acesso por projeto
Memória | Memória em Markdown na sua pasta de usuário ou memória de longo prazo do MemoryMesh, recuperada antes de cada turno e exibida no processo de trabalho
```

### Segurança e revisão

```ui-checklist
ok | Modos de permissão | Somente leitura, Gravação no espaço de trabalho, Aprovação automática e Acesso total
ok | Fluxo de aprovação | Regras, um modelo de avaliação e um modelo revisor; tudo que for incerto vai para você
ok | Modelo de avaliação | Roda em um modelo local, na nuvem ou por uma API
ok | Revisão de diff | Visualização unificada e dividida, mudanças por palavra, preparação (staging) e reversão
ok | Git | Pull, mensagens de commit escritas pela IA, commit no branch atual ou em um novo e depois push
```

### Ampliando o Vulcan Code

```ui-cards
Habilidades | Habilidades de prompt (SKILL.md) e pacotes programáveis do LuaSkills, instaláveis pelo LuaSkills Hub
MCP | Conecte servidores MCP locais e HTTP ou deixe a IA conectar um apenas para uma sessão
Serviço MCP | Outras ferramentas de IA podem listar sessões, enviar mensagens e responder perguntas, cada uma com seu próprio token
Configuração da IA | A IA pode adicionar servidores MCP e provedores de modelos para você, mostrando antes uma prévia e perguntando conforme o seu modo de permissão exigir
Plugins | Plugins adicionam ferramentas e páginas próprias, como o navegador de IA e as estatísticas de uso
RTK | Comprime a saída de comandos e o contexto do Git para economizar tokens e contabiliza a economia
Modelos | OpenAI, Anthropic, Gemini e outras famílias de protocolos, provedores integrados e personalizados, com modelos separados para cada finalidade
```

```ui-callout info Versão prévia
A 0.1.x ainda é uma prévia, e os recursos e a interface vão continuar mudando. Na atualização, o banco de dados do aplicativo e os dados das sessões passam primeiro por backup e depois são atualizados automaticamente versão a versão, sem que você precise fazer nada.
```
