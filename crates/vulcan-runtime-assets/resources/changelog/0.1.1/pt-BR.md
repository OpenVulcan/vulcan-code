O Vulcan Code 0.1.1 traz atualizações dentro do app, uma nova forma de instalar e um serviço de usuário opcional.

### Atualizações dentro do app

```ui-checklist
ok | Verificar e baixar | O serviço procura novas versões, baixa só os arquivos que mudaram e só troca de versão depois que a verificação de assinatura passa
ok | Reinício ao confirmar | O app para desktop, a WebUI no navegador e a TUI mostram as atualizações disponíveis e reiniciam na nova versão quando você confirma
ok | Reversão automática | Se uma nova versão não iniciar, o inicializador volta para a anterior e o app avisa você
ok | Atualizações completas | Quando é preciso uma instalação completa, o app baixa e executa o instalador e abre de novo ao terminar
```

### Instalação e serviço

```ui-cards
Programa de instalação | O novo vulcan-code-setup instala por usuário no Windows, macOS e Linux, com app para desktop, cliente de terminal, entradas de menu e atalho na área de trabalho opcionais
Serviço de usuário | O serviço pode iniciar quando você entra na sessão: item de inicialização do Windows, LaunchAgent do macOS ou serviço de usuário do systemd no Linux
Faixa de portas | As portas usadas pelo serviço podem ser alteradas nas configurações avançadas do programa de instalação
Cliente de terminal | Quando nenhum serviço está em execução, a TUI inicia o serviço instalado, como o app para desktop
```

```ui-callout info Atualizar a partir da 0.1.0
A 0.1.0 ainda não tem atualizações dentro do app: execute de novo o instalador ou o comando de instalação para ir para a 0.1.1; as versões seguintes se atualizam dentro do app.
```
