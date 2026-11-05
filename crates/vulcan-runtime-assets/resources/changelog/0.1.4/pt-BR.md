O Vulcan Code 0.1.4 adiciona um jeito rápido de atualizar, mostra na barra de status o que a sua conta de modelo informa e mantém as caixas de diálogo utilizáveis em telas pequenas.

### Atualizações

```ui-checklist
ok | Botão de atualização | Quando há uma versão mais nova, aparece um botão de atualização ao lado do nome Vulcan Code na barra lateral; ele mostra as novidades e baixa e reinicia sem abrir as configurações
ok | Versões puladas | Quando uma atualização pula versões, a prévia mostra as notas de cada versão pulada, da mais nova para a mais antiga; vale para atualizações a partir da 0.1.4
```

### Conversa e barra de status

```ui-checklist
ok | Conversa estável | Os avisos de envio de solicitação, recuperação de memória e preparação de uma chamada de ferramenta agora flutuam acima da caixa de entrada, então a conversa não pula mais quando eles mudam
ok | Conta de modelo | A barra de status mostra o saldo da conta, uma pequena barra com a cota restante ou os dois; uma conta que não informa nenhum deles mostra só o nome, em vez de "Saldo -"
```

### Telas pequenas

```ui-checklist
ok | Caixas de diálogo | Caixas de diálogo com botões na parte de baixo rolam o conteúdo em telas baixas, para que os botões continuem ao alcance, incluindo a configuração rápida da memória de longo prazo e as aprovações do Ask
ok | Configuração rápida da memória de longo prazo | A etapa de modelos mostra cada finalidade em uma linha, então todas cabem em uma tela
```

### Git

```ui-checklist
ok | Pull | Quando a branch local e o upstream avançaram os dois, o pull explica isso com o número de commits de cada lado, em vez de mostrar as dicas do Git
ok | Configurações de commit | Depois de salvar o modelo das mensagens de commit, a página não indica mais alterações não salvas
```
