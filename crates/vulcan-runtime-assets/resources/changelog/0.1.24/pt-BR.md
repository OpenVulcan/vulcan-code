O Vulcan Code 0.1.24 melhora os anexos e a recuperação ao trocar de modelo, além de corrigir problemas no navegador integrado e nos logs do terminal.

### Conversas e entrada

```ui-checklist
ok | Arrastar e colar arquivos | A visualização de anexos foi restaurada. Arquivos que o modelo não pode enviar podem virar referências quando o caminho original está disponível. As referências enviadas podem ser expandidas fora do corpo da mensagem.
ok | Troca de modelo e recuperação | O histórico original é preservado. Se não for possível reproduzir o raciocínio com segurança, copie as mensagens visíveis para o rascunho de uma nova conversa e revise antes de enviar.
ok | Compactação e rolagem | A compactação em andamento aparece separadamente. Novos trechos ficam visíveis enquanto a tela acompanha o fim da conversa.
```

### Navegador integrado

```ui-checklist
ok | Exibição e edição | Corrigimos o carregamento de estilos e recursos de janelas pop-up e melhoramos a edição. As senhas continuam ocultas no navegador integrado quando a página altera sua visibilidade.
```

### Logs e provedores

```ui-checklist
ok | Logs do terminal | Aprimoramos a ocultação de valores secretos em quebras de linha e saídas truncadas. Ainda é possível parar tarefas em segundo plano quando a ocultação estiver indisponível.
ok | Erros do provedor | Os cartões mantêm mensagens legíveis e links de ajuda, ocultando URLs que contêm credenciais.
ok | Compatibilidade com provedores | O Token Plan do Alibaba Cloud Model Studio deixou de tentar uploads temporários para OSS. Corrigimos solicitações de bases de conhecimento que usam embeddings da OpenVulcan.
```
