O Vulcan Code 0.1.22 adiciona memória gratuita na nuvem e melhora o envio de imagens em conversas longas, os avisos de erro e a gravação de voz.

### Memória

```ui-checklist
ok | Memória gratuita na nuvem | Entre no OpenVulcan em um site compatível. Antes de ativar, consulte os modelos, as regiões de processamento e o tratamento de dados. O uso aparece por etapa.
ok | Reconstrução de vetores | Veja os lotes concluídos e a contagem regressiva quando houver limite de frequência. A reconstrução continua após a espera.
ok | Memória Markdown | Os conflitos distinguem a revisão provisória do lote da última revisão salva.
```

### Conversas

```ui-checklist
ok | Novas imagens em conversas longas | Com a compactação automática ativada, as imagens anteriores podem ser resumidas em lotes. As novas imagens são enviadas na solicitação original.
ok | Erros de provedores | Erros de cota do Bailian são tratados como limites de frequência. Falhas de saldo ou rede aparecem em cartões de erro, e avisos comuns do sistema aparecem como linhas de status.
```

### Entrada de voz e ferramentas

```ui-checklist
ok | Duração da gravação | Gravações que atingem o limite de duração podem ser enviadas para transcrição.
ok | Estatísticas do RTK | O ranking de comandos usa o banco de estatísticas do RTK existente e dispensa a leitura dos registros de conversas do Claude.
```
