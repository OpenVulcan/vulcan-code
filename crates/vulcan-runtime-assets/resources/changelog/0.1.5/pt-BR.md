O Vulcan Code 0.1.5 dá a cada modelo sua própria janela de compactação padrão, inicia o aplicativo de desktop em uma única janela e permite escolher com que frequência procurar atualizações.

### Janela de contexto

```ui-checklist
ok | Padrão por modelo | Cada modelo de conversa ativado pode ter sua própria janela de compactação padrão para novas sessões: o padrão do sistema, seguir o modelo, um nível fixo ou um tamanho personalizado; as sessões existentes não mudam
ok | Janelas personalizadas | Um tamanho personalizado aceita uma quantidade de tokens ou K para milhares (como 235K), no mínimo 100K e no máximo a janela do modelo; o seletor de janela ao lado da caixa de entrada também pode mudar para um tamanho personalizado
ok | Trocar de modelo | Ao trocar de modelo em uma sessão, a janela padrão do modelo de destino é oferecida e nada muda até você confirmar; uma janela que o modelo de destino não comporta passa a seguir o modelo em vez de impedir a troca
ok | Interface de terminal | As configurações da TUI também permitem definir a janela padrão por modelo
```

### Atualizações

```ui-checklist
ok | Intervalo de verificação | Escolha com que frequência procurar atualizações automaticamente: a cada hora, a cada 6 horas, a cada 12 horas ou todo dia, a cada 6 horas por padrão; a mudança vale sem reiniciar
ok | Página de atualização de software | A página de atualização de software nas Configurações foi reorganizada: primeiro a versão atual com verificar e reparar, depois as configurações de atualização e por último a atualização encontrada com as notas de cada versão que ela traz, da mais recente para a mais antiga; o diálogo de atualização mostra a mesma lista de versões
ok | Reinício no Linux | No Linux, um Core iniciado pelo aplicativo de desktop reinicia após uma atualização em vez de ficar parado em "Reiniciando"
```

### Aplicativo de desktop

```ui-checklist
ok | Início em uma janela | Não abre mais uma janela de início separada; a janela de trabalho mostra a tela de início com as etapas reais e qualquer falha, e abre a interface no mesmo lugar quando o Core está pronto
ok | Aparência ao iniciar | A tela de início usa o idioma e a aparência da última vez; o primeiro início segue o sistema
ok | Ícone do aplicativo | O dock do Linux e o Dock do macOS mostram o ícone do Vulcan Code em vez de um genérico; abrir o aplicativo de novo traz para a frente a janela que está iniciando
```

### Correções

```ui-checklist
ok | Modelos de embeddings | Modelos de embeddings com tamanho de saída ajustável (como Qwen3-Embedding-8B, de 32 a 4096 dimensões) não rejeitam mais todas as respostas
ok | Componentes de resposta | Avisos sem tom e linhas de constatações com quatro células agora são exibidos em vez de voltarem a ser blocos de código, na WebUI e na TUI
```

### Instalador

```ui-checklist
ok | Instalação em uma linha no macOS | Executar o script de instalação por um pipe do curl no macOS agora consegue ler o teclado
ok | Aplicativo para macOS | O aplicativo para macOS criado pelo instalador tem ícone, e o instalador informa onde ele está ao terminar
ok | Item de menu do Linux | O item de menu do Linux corresponde à janela em execução, então um ícone fixado no dock abre o aplicativo
```
