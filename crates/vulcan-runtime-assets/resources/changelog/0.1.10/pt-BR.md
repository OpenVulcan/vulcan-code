O Vulcan Code 0.1.10 corrige a edição e a bifurcação em sessões longas, mantém os turnos diante de limites de taxa e argumentos de ferramenta malformados e melhora os cartões de comando e os componentes de resposta.

### Atualizar para esta versão

```ui-checklist
ok | Atualização incremental | Atualizar a partir da 0.1.8 ou 0.1.9 baixa apenas os arquivos alterados; versões anteriores continuam sendo instaladas como atualização completa
ok | Instalações pelo instalador | Instalações feitas com o programa de instalação são atualizadas com vulcan-code-setup update; o app informa isso em vez de oferecer um download
```

### Sessões

```ui-checklist
ok | Edição e bifurcação | Em sessões longas cujo histórico abrange vários segmentos, editar uma mensagem enviada ou bifurcar a partir de um turno não falha mais com o erro provider_source fingerprint mismatch
ok | Argumentos de ferramenta malformados | Uma chamada de ferramenta com argumentos ilegíveis recebe um resultado de erro e o turno continua, em vez de terminar no segundo erro; argumentos cortados são indicados como provavelmente longos demais
ok | Limites de taxa | Limites HTTP 429 são repetidos conforme o calendário de limites de taxa quando "Aguardar e tentar novamente ao atingir o limite de taxa" está ativado, em vez de encerrar o turno; saldo esgotado ainda interrompe
```

### Ferramentas e respostas

```ui-checklist
ok | Títulos de comando | O argumento de descrição do shell_command passa a ser um título curto de no máximo 80 caracteres, o que evita que alguns modelos, como o MiniMax-M3.1-Flash, quebrem a chamada nesse argumento
ok | Cartões de tarefas em segundo plano | Os cartões de shell_job mostram a finalidade do comando após o ID da tarefa, para saber qual comando está sendo aguardado; o cliente de terminal intitula as linhas de comando e de tarefa da mesma forma
ok | Componentes de resposta | Uma terceira célula em uma linha ui-kv aparece como nota abaixo do valor, e uma variação com sinal como +5 em ui-metrics aparece como variação, em vez de o componente inteiro voltar a ser código
```

### Modelos e memória

```ui-checklist
ok | OpenCode Go DeepSeek | deepseek-v4-pro e deepseek-v4.1-flash podem desligar o raciocínio e, por isso, podem ser escolhidos como modelos de memória
ok | Nota do modelo de memória | As configurações agora explicam por que só aparecem modelos que podem desligar o raciocínio: o modelo de memória é chamado antes e depois de cada resposta, e o raciocínio somaria espera e consumo a cada vez
```
