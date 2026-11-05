# Teste nativo de Remote Config

O teste verifica se a API pública Rust consegue consultar o Remote Config real
da Qonversion em iOS e Android sem interface gráfica. O contrato compartilhado
inicializa o SDK em Sandbox, consulta `remote_config("dioxus_integration_test")`
e valida os tipos e valores retornados.

| Campo | Tipo | Valor |
| --- | --- | --- |
| `feature_test_enabled` | booleano | `true` |
| `test_id` | string | `dioxus_ios_remote_config` |
| `message` | string | `Remote Config funcionando` |

Erro do SDK, timeout, payload vazio ou qualquer campo ausente ou incorreto
fazem o contrato falhar.

## Execução no GitHub Actions

O workflow **Native Remote Config** roda em pushes na branch
`test/qonversion-remote-config-ios`. O Cargo inclui as pontes nativas com a
feature `native-sdk-test`. Cada job também habilita a execução no seu host.

- **iOS** liga a biblioteca Rust estática ao XCTest e executa o teste no
  Simulator com `QONVERSION_RUN_NATIVE_REMOTE_CONFIG=1` no plano de teste.
- **Android** carrega a biblioteca Rust via JNI e roda o JUnit de instrumentação
  em um emulador API 34 com `-PrunNativeRemoteConfig=true`. O JUnit mantém uma
  Activity transparente e sem conteúdo durante o teste. Ela coloca o processo
  em primeiro plano, evento que o SDK exige para concluir a inicialização
  interna e liberar a consulta.

O workflow usa `run.py` para validar os relatórios nativos. Cada job exige
exatamente um teste aprovado, sem falhas nem testes ignorados. Um relatório
ausente também falha. Em caso de falha, o workflow exibe diagnósticos
sanitizados e mantém os arquivos brutos no diretório temporário do job.

### Configurar a chave

No repositório, abra **Settings → Environments**, selecione ou crie o
Environment `test` e adicione a SDK Project Key como secret
`QONVERSION_PROJECT_KEY`. O workflow lê o secret na compilação Rust e na
sanitização de logs. Mantenha o valor fora do código e dos relatórios.

No commit `0936a96`, o iOS aprovou um XCTest e o Android aprovou um JUnit.
Os dois validaram os três campos com o SDK real.
[Execução aprovada](https://github.com/nichmorgan/dioxus-qonversion/actions/runs/42111727584)

## Verificação local

```sh
cargo test --locked --offline --lib -j 1
python -B -m unittest discover -s integration-tests/native-remote-config -p 'test_*.py' -v
```

Esses comandos verificam a biblioteca e os auxiliares. Para rodar a integração
real localmente, forneça `QONVERSION_PROJECT_KEY` no ambiente e execute
`run.py ios --run` ou `run.py android --run`, com `--device` para selecionar
um simulador ou emulador já iniciado.
