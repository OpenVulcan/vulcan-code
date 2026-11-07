O Vulcan Code 0.1.11 adiciona feedback, uma central de mensagens e uma forma de corrigir problemas no seu próprio dispositivo, e atualiza os modelos da OpenAI, TokenRhythm, OpenCode Go e OpenDesign.

### Atualizar para esta versão

```ui-checklist
ok | Atualização incremental | Atualizar a partir da 0.1.8, 0.1.9 ou 0.1.10 baixa apenas os arquivos alterados; versões anteriores continuam sendo instaladas como atualização completa
ok | Banco de dados do app | Na primeira inicialização, o banco de dados do app é copiado como backup e depois atualizado automaticamente para a nova estrutura, sem nenhuma ação sua
```

### Feedback e mensagens

```ui-checklist
ok | Enviar feedback | Com a sessão iniciada, "Enviar feedback" permite relatar um problema ou fazer uma sugestão; antes do envio, segredos, caminhos, seu nome de usuário, URLs e dados pessoais são filtrados no dispositivo e você vê exatamente o conteúdo que será enviado
ok | Relatar a partir de um cartão de erro | Quando uma conversa falha, o cartão de erro oferece um relato com um clique; só para falhas que o Vulcan Code pode causar, não para login, saldo, limites de taxa, indisponibilidade do serviço, tempo esgotado, política de conteúdo ou cancelamento
ok | Central de mensagens | A "Central de mensagens" mostra anúncios, mensagens pessoais e respostas dos mantenedores ao seu feedback; anúncios importantes também aparecem como banner
ok | Corrigir neste dispositivo | Escolha uma pasta e uma cópia do vulcan-code é preparada como projeto, com uma conversa que tem a habilidade de correção; nada é enviado nem publicado até você confirmar o pull request nessa conversa
ok | Entradas do GitHub | O menu da conta ganha "Sugerir no GitHub (issue)" e "Abrir um pull request no GitHub"
ok | Cliente de terminal | O cliente de terminal também tem a página de feedback
```

### Modelos e provedores

```ui-checklist
ok | OpenAI | Oito modelos da API pública declaram saída estruturada estrita com json_schema; GPT-5.6 Sol/Terra/Luna, GPT-6 Sol/Luna e GPT-6.1 Terra podem desligar o raciocínio
ok | TokenRhythm | Adiciona DeepSeek V4.1 Flash 1017, LongCat 2.0, MiniMax M3.1 Flash e os modelos de teste Egg-Alpha e Neohorse 1 9B, e remove minimax-m2.5 e mimo-v2.5-pro; a saída estruturada é declarada, os modelos Messages agora enviam a intensidade do raciocínio e podem desligá-lo, DeepSeek V4 Pro 0813 e GLM-5.2 passam para Responses e Kimi K2.6 para Chat
ok | OpenCode Go | Adiciona Gemini 4 Argon, Hy4 e os modelos gratuitos Beam, Iceman e Crouton; dez modelos declaram saída estruturada
ok | OpenDesign | GPT-6 Luna agora usa Responses e Claude Haiku 5.5 usa Messages; adiciona deepseek-v4.1-flash-eco, hy4-preview, minimax-m3.1-flash e 14 modelos de chat Plus/Pro/Max; a geração de imagens com GPT Image 2, Nano Banana 2/2 Lite e Seedream 5.0/5.0 Pro está disponível nos planos que a incluem; os níveis de raciocínio mostram apenas o que cada fornecedor publica
ok | Modelos de memória | Mais modelos podem desligar o raciocínio e retornar saída estruturada, então mais deles podem ser escolhidos como modelos de memória
```
