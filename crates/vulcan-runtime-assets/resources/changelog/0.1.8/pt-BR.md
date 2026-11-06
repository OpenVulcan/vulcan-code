O Vulcan Code 0.1.8 executa comandos de shell como tarefas que podem continuar em segundo plano, adiciona uma página inicial do projeto, deixa o ícone da bandeja abrir o cliente que você escolher e trocar o idioma, e traz várias centenas de correções de uma revisão completa antes do lançamento. Vem com o programa de instalação 0.2.10.

### Atualizar para esta versão

```ui-checklist
ok | Atualização completa | Esta versão muda o instalador de novo, então é instalada como atualização completa: no Windows o app baixa e executa o instalador uma vez
ok | Instalações pelo setup | Instalações feitas com o programa de instalação são atualizadas com vulcan-code-setup update; o app avisa isso em vez de oferecer um download
```

### Comandos de shell

```ui-checklist
ok | Saída ao vivo | Um cartão de comando expandido mostra as linhas mais recentes de um comando em execução, então comandos longos não parecem mais travados
ok | Tarefas em segundo plano | Um comando pode rodar em segundo plano; uma mensagem enviada enquanto ele roda o leva para lá, e também dá para movê-lo pelo cartão do comando
ok | Tarefas concluídas | Quando um comando em segundo plano termina depois que a IA parou, a sessão continua sozinha; o término aparece como um cartão de contexto com o nome e o resultado do comando
ok | Sem limite de tamanho | A saída vai para um log rotativo em vez de parar o comando em 8 MiB; a IA recebe as linhas mais recentes e pode ler as anteriores quando precisar
ok | Painel de atividade | O painel da sessão conta subagentes e comandos em segundo plano e abre uma lista com os logs; reiniciar para atualizar avisa sobre tarefas em execução
ok | Sempre visível | As contagens aparecem em todas as sessões, mesmo sem Git, subagentes ou comandos em execução
```

### Projetos

```ui-checklist
ok | Início do projeto | O nome do projeto na barra lateral abre uma página do projeto com renomear, ações rápidas, estatísticas de sessões, uma lista de sessões pesquisável e todas as configurações do projeto
ok | Edições não salvas | Edições não salvas da memória do projeto são mantidas ao sair da página do projeto e voltar
```

### Bandeja

```ui-checklist
ok | Clique duplo | No Windows, um clique duplo no ícone da bandeja abre o cliente que você escolher: o app de desktop, a WebUI ou o cliente de terminal; o menu passa para o botão direito
ok | Idioma | Um menu de idioma na bandeja troca o idioma da interface, e os clientes abertos acompanham
```

### Interface

```ui-checklist
ok | Largura de leitura | Todas as larguras predefinidas da conversa ficaram maiores, e uma coluna mais larga não recolhe mais a navegação em janelas comuns
ok | Indicador de status | O indicador de status agora fica na borda superior da caixa de entrada, em vez de flutuar sobre a conversa
ok | Temas escuros | O texto selecionado fica visível nos temas escuros
ok | Componentes de resposta | Chaves longas não espremem mais os valores numa coluna estreita, e fórmulas em vários grupos antes de uma legenda aparecem corretamente
ok | Execuções de ferramentas | Cartões de ferramentas em execução dentro de uma execução recolhida ficam juntos
```

### Correções

```ui-checklist
ok | Modo de plano | Só um plano enviado aguarda confirmação, e ele é restaurado depois de reiniciar
ok | Respostas vazias | Uma resposta que termina sem conteúdo é repetida conforme suas configurações de nova tentativa; se o limite de saída cortá-la antes de qualquer resposta, isso é informado
ok | Continuar uma sessão | Enviar uma mensagem depois da organização do contexto não falha mais ao iniciar
ok | Parâmetros do modelo | Um parâmetro definido para certos modelos não é enviado a outros modelos
ok | MCP | Listas de ferramentas em várias páginas são lidas por completo, e cabeçalhos definidos por você têm prioridade sobre os de variáveis de ambiente
ok | Instalador do Windows | Se a troca para a nova versão falhar, o instalador mostra o motivo e mantém os arquivos
ok | Instalação danificada | Um app de desktop com instalação incompleta avisa disso em vez de iniciar o serviço errado
ok | Correções da revisão | Várias centenas de correções menores em sessões, armazenamento, memória, arquivos, permissões, plugins, WebUI e cliente de terminal
```

### Segurança

```ui-checklist
ok | Navegador integrado | O navegador integrado bloqueia todas as formas de endereço local da porta protegida do Core
ok | Erros do provedor | As chaves de API são removidas dos erros do provedor antes de serem encurtados ou exibidos
ok | Ferramentas de plugins | Argumentos que não correspondem aos parâmetros de uma ferramenta de plugin são recusados antes de ela rodar
```

### Cliente de terminal

```ui-checklist
ok | Atividade | Uma aba de atividade, um contador na barra de sessão e uma visualização de log para comandos em segundo plano, além de um aviso antes de reiniciar
ok | Correções | Seletores, colar em respostas a perguntas, seleção com o mouse, menus longos e blocos de código funcionam corretamente
```

### Programa de instalação

```ui-checklist
ok | Nova tentativa após falha | Executar o setup de novo depois de uma troca de versão que falhou registra o serviço outra vez
ok | Várias instalações | Desinstalar uma instalação não mexe nos iniciadores, itens de menu e serviços de outra
```
