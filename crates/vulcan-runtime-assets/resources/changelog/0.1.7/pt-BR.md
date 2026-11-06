O Vulcan Code 0.1.7 permite abrir o app de desktop do Linux pelo dock e pelo menu de aplicativos no Ubuntu 23.10 e posteriores, torna reinstalações e atualizações no Windows mais seguras, corrige várias falhas de segurança e de perda de dados e vem com o programa de instalação 0.2.9.

### Atualizar para esta versão

```ui-checklist
ok | Atualização completa | Esta versão muda o instalador e a estrutura da instalação, então é instalada como atualização completa: no Windows o app baixa e executa o instalador uma vez
ok | Instalações pelo setup | Instalações feitas com o programa de instalação são atualizadas com vulcan-code-setup update; o app avisa isso em vez de oferecer um download
```

### Linux

```ui-checklist
ok | Dock e menu de aplicativos | No Ubuntu 23.10 e posteriores, o setup 0.2.9 instala uma vez um perfil do AppArmor, então o app de desktop abre pelo dock e pelo menu de aplicativos, e não só por um ícone na área de trabalho
ok | Perfil pelo app | Quando o perfil falta ou está desatualizado, as configurações de atualização oferecem instalá-lo pedindo a senha de administrador uma vez, ou mostram o comando para executar em um terminal
```

### Windows

```ui-checklist
ok | Reinstalação segura | Instalar em uma pasta sem instalação anterior nunca apaga os arquivos dela; quando a reinstalação da mesma versão não termina, a anterior é restaurada
ok | Inicialização de teste | Abrir o app de desktop enquanto o Core anterior ainda está rodando não conta mais como falha ao iniciar a nova versão nem a reverte
```

### Segurança

```ui-checklist
ok | Serviço local | O Core só aceita solicitações endereçadas a um nome de host de loopback, então uma página da web que aponte o próprio endereço para este computador não consegue ler o serviço
ok | Anexos | Anexos SVG abrem em um sandbox e não podem executar scripts, e os caminhos dos anexos não podem sair da pasta deles
ok | Verificação de somente leitura | Ferramentas de extensões com um verbo de escrita no nome, as opções de escrita do git branch e as opções de execução do fd não são mais tratadas como somente leitura
ok | Segredos | Gravar um valor comum sobre uma variável de ambiente secreta a mantém secreta
```

### Correções

```ui-checklist
ok | Skills e projetos | Instalar um skill local a partir da própria pasta não o apaga mais, e excluir um projeto também remove as variáveis do projeto
ok | Memória | A memória em Markdown lê corretamente títulos em chinês, japonês e coreano
ok | Modo de plano | Ligar ou desligar o modo de plano depois que um turno terminou não falha mais
ok | Parâmetros de ferramentas | Parâmetros que se referem a definições compartilhadas, como listas de valores permitidos, continuam visíveis e obrigatórios
ok | Parâmetros do modelo | Parâmetros decimais são enviados como números, e a extração do Tavily funciona só com URLs
ok | Edição de arquivos | Inserir no fim de um arquivo mantém a última linha e as linhas em branco finais
ok | WebUI | As setas na mensagem de commit movem o cursor, linhas do diff que começam com --- ou +++ aparecem, e exemplos de código mantêm os links de imagem como foram escritos
```

### Cliente de terminal

```ui-checklist
ok | Prompts de acompanhamento | Prompts para uma sessão existente são aceitos, e um prompt que não é aceito volta para a caixa de entrada
ok | Git | Uma conexão perdida não trava mais o commit e a geração da mensagem até reiniciar
ok | Níveis de raciocínio | Cada nível de raciocínio é enviado como foi escolhido
```

### Programa de instalação

```ui-checklist
ok | Perfil do AppArmor | O setup 0.2.9 instala o perfil via sudo quando o sistema precisa e deixa o comando exato nas notas quando a senha não é informada; a desinstalação o remove
```
