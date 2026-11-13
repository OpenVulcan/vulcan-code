O Vulcan Code 0.1.23 adiciona a limpeza protegida do histórico, melhora o navegador integrado e corrige o carregamento dos módulos Lua incluídos no Linux e no macOS.

### Atualização para esta versão

```ui-checklist
ok | Instalador 0.2.12 | O script reutiliza o instalador existente, que se atualiza e reinicia automaticamente. No Linux e no macOS, a integração com o PATH disponibiliza os comandos em novos terminais. A instalação por linha de comando mostra os comandos do serviço e as portas da WebUI.
```

### Conversas

```ui-checklist
ok | Limpeza rápida do histórico | Em uma conversa comum e ociosa que já passou pela compactação, use o menu de adição ou `/clean-history` para escolher até qual turno limpar. Os detalhes antigos de ferramentas e raciocínio são removidos. Perguntas e respostas simplificadas continuam na linha do tempo, e o histórico ainda necessário fica protegido.
ok | Rolagem em conversas longas | O Markdown em transmissão é atualizado de forma independente. A linha do tempo mantém a posição de leitura ao carregar turnos anteriores, inclusive o histórico limpo.
```

### Navegador integrado

```ui-checklist
ok | Renderização de páginas | A WebUI usa a estrutura das páginas do navegador integrado para renderizá-las, em substituição à transmissão de capturas de tela.
ok | Tamanhos da área de visualização | Escolha um tamanho para desktop, retrato ou paisagem, informe dimensões personalizadas ou ajuste à janela atual com uma só ação.
```

### Área de trabalho e ambiente de execução

```ui-checklist
ok | Base de conhecimento | Novas barras laterais fixam a base de conhecimento por padrão. A disposição salva tem prioridade.
ok | Ambiente Lua no macOS | Corrigido o carregamento dos módulos Lua nativos incluídos em Macs Intel e com Apple Silicon. O Context7 inicializa o armazenamento local ao abrir as configurações pela primeira vez.
ok | Ambiente Lua no Linux | Os módulos Lua nativos incluídos, como OpenSSL, agora carregam em ARM64 e x64. O empacotamento da versão verifica a compatibilidade com glibc 2.35.
```
