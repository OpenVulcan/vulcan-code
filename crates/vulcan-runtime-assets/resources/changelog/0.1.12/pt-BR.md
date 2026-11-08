O Vulcan Code 0.1.12 conclui dentro do app as atualizações que exigem reinstalação e corrige os botões de janela e o ícone no macOS, além das solicitações de título e validação que alguns provedores rejeitavam.

### Atualizar para esta versão

```ui-checklist
ok | Atualização incremental | Atualizar a partir da 0.1.8 até a 0.1.11 baixa apenas os arquivos alterados; versões anteriores continuam sendo instaladas como atualização completa
ok | Reinstalação dentro do app | As próximas atualizações que exigirem reinstalação não levam mais você ao site: apenas os arquivos alterados são baixados, os recursos básicos já instalados, como o navegador, o Electron e o Node, são reaproveitados, e o programa de instalação conclui a atualização ao reiniciar
ok | Cópias instaladas pelo instalador do Windows | Uma cópia instalada pelo instalador do Windows recebe essas atualizações como um pequeno pacote de atualização assinado em vez do instalador completo
```

### Interface

```ui-checklist
ok | Botões de janela no macOS | Ficam apenas os botões de janela do sistema, então minimizar, maximizar e fechar não aparecem mais em dobro, e o botão verde volta a oferecer o menu de lado a lado e tela cheia do sistema
ok | Ícone no macOS | O ícone no Dock e com o app fechado tem o mesmo tamanho dos ícones de outros apps
ok | Cartões de contas de serviços de modelos | Os cartões têm uma largura mínima que acompanha a escala da interface, então as linhas de cota não quebram mais em várias linhas
ok | Assistente de configuração | Com a sessão do OpenVulcan iniciada e nenhum modelo avaliador configurado, a etapa de avaliação pré-seleciona o avaliador na nuvem; nada é salvo até você confirmar
```

### Modelos e provedores

```ui-checklist
ok | Solicitações corrigidas | Os títulos de sessão, a validação de modelos nas configurações e o rascunho de prompts dos assistentes dedicados agora enviam um escopo de cache, então o OpenCode Go não retorna mais 400 e o Codex Responses Lite não os rejeita mais
ok | Plataformas em nuvem | CommandCode, OpenCode Go, Open Design, OpenRouter e TokenRhythm são classificados como plataformas em nuvem; as informações compartilhadas dos modelos vêm primeiro dos provedores oficiais, depois das plataformas em nuvem e por fim dos intermediários
ok | Lista de modelos base | O que um modelo consegue fazer e seus limites ficam registrados uma única vez em uma lista de modelos base herdada pelas regras dos provedores; a página de status dos manifestos de provedores indica os campos herdados dela
```
