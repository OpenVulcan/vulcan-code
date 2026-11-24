O Vulcan Code 0.1.30 adiciona o modo Economia, o login pelo plano ChatGPT e a transferência de configurações. Também melhora o MCP, o navegador integrado e a recuperação de mensagens na fila.

### Sessões e contexto

```ui-checklist
ok | Modo Economia por sessão | Defina uma janela de histórico para sessões comuns. Por padrão, após 40 turnos concluídos, os 20 turnos elegíveis mais antigos podem sair das próximas solicitações ao modelo. O histórico completo continua visível. Ativar o modo em uma sessão existente pode fazer uma solicitação adicional ao modelo.
ok | Enviar mensagens da fila após uma falha | Após o encerramento do turno com falha, envie a mensagem selecionada com seus anexos e ajustes de modelo originais. O inspetor mostra resumos, o histórico mantido e a expiração da memória.
```

### Contas e modelos

```ui-checklist
ok | Acessos separados da OpenAI | Conecte separadamente contas da API OpenAI, do Codex e do plano ChatGPT. O login e a consulta de cotas do Codex voltaram a funcionar via HTTP. Anexos e algumas respostas em streaming do plano ChatGPT foram corrigidos.
ok | Transferência seletiva de configurações | Exporte as credenciais e os provedores personalizados selecionados em um arquivo `.vc_mcfg` protegido por senha. Contas com tokens rotativos exigem novo login no dispositivo de destino.
ok | Uso e edição de imagens | Consulte o uso do Step Plan da StepFun nas duas regiões. Edite imagens com GPT Image 2 ou 2.5 pela API OpenAI, ou use imagens de referência com GPT Image 2 pelo Codex.
```

### Ferramentas e área de trabalho

```ui-checklist
ok | Atalhos de MCP e LuaSkills por projeto | Restrinja os servidores MCP a projetos selecionados e use-os sob demanda. Escolha uma opção disponível no menu `#` do campo de entrada. As permissões da sessão continuam valendo.
ok | Interação direta com o navegador | Interaja diretamente com as páginas do navegador integrado. Conversas longas preservam melhor a posição de leitura, e os cartões de comandos exibem o tempo decorrido e o limite.
```

### Atualização

```ui-checklist
ok | Atualização automática de dados | Ao abrir o banco de dados e sessões antigas, o aplicativo atualiza o formato e guarda uma cópia anterior. Sessões comuns existentes permanecem no modo padrão até o modo Economia ser ativado.
ok | Instalação e nova tentativa no Windows | O instalador 0.2.19 instala o runtime Visual C++ x64 quando ele está ausente. Se a verificação de inicialização falhar, os arquivos verificados são mantidos e apenas o conteúdo ausente ou danificado é baixado novamente.
```
