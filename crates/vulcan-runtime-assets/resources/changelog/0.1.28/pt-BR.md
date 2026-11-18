O Vulcan Code 0.1.28 adiciona espaços de trabalho próprios e histórico de chamadas para clientes MCP externos, além de atualizar no início as habilidades ROOT gerenciadas que estiverem abaixo da versão exigida.

### MCP externo

```ui-checklist
ok | Projetos e sessões próprios | Clientes externos podem registrar diretórios de trabalho e criar suas próprias sessões. As ferramentas de arquivos, CodeKit, shell e bases de conhecimento usam o espaço vinculado ao ID da sessão.
ok | Comandos em segundo plano e terminais | Com as permissões correspondentes, os clientes podem executar comandos em segundo plano, consultar ou parar tarefas e usar sessões de terminal.
ok | Histórico opcional de chamadas | Ative o registro por cliente para consultar argumentos, resultados e status. O histórico pode ser filtrado por cliente, projeto ou sessão.
```

### Atualização

```ui-checklist
ok | Chamadas MCP independentes exigem um ID de sessão | Registre um projeto com `vc_mcp_project`, crie uma sessão com `vc_mcp_session` e passe seu `sid` às ferramentas independentes. A sessão determina o diretório de trabalho.
```

### Habilidades e verificações de desenvolvimento

```ui-checklist
ok | Atualização de habilidades ROOT na inicialização | CodeKit, Vulcan Curl e Vulcan Work são atualizados a partir da origem configurada quando a versão instalada é inferior à mínima. Se falhar, a versão utilizável é mantida e a atualização é tentada no próximo início.
ok | Teste de aceitação HTTP para políticas de VMM | `make test-vmm-http` verifica por HTTP a interação dos controles de política da WebUI com um serviço local.
```
