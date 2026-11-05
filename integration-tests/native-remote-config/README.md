# Teste nativo de Remote Config

O workflow **Native Remote Config** executa um único contrato Rust pelo XCTest no
iOS e pelo JUnit/JNI no Android. Cada teste inicializa a Qonversion com o SDK real,
consulta `remote_config("dioxus_integration_test")` e verifica estes valores e tipos:

| Campo | Tipo | Valor |
| --- | --- | --- |
| `feature_test_enabled` | booleano | `true` |
| `test_id` | string | `dioxus_ios_remote_config` |
| `message` | string | `Remote Config funcionando` |

Erros do SDK, timeout, payload vazio e campos ausentes, de tipo incorreto ou com
valor diferente falham no Rust. Cada job também exige o relatório de exatamente
um teste executado e aprovado. Relatórios ausentes, testes ignorados e falhas
impedem a aprovação.

## Execução no GitHub Actions

Um push na branch `test/qonversion-remote-config-ios` aciona os jobs independentes.
O Cargo compila com a feature explícita `native-sdk-test`. No iOS, o plano XCTest
recebe `QONVERSION_RUN_NATIVE_REMOTE_CONFIG=1`. No Android, o Gradle recebe
`-PrunNativeRemoteConfig=true`. Essas flags habilitam o teste no respectivo host.

- **iOS** usa Xcode 16.4 e o Simulator iOS 18.5. O job verifica o arquivo Rust
  estático com `ar` e `lipo`, liga seu caminho absoluto ao projeto XCTest e
  executa a classe de teste selecionada com `xcodebuild`.
- **Android** usa API 34 x86_64 com KVM e renderização SwiftShader. A action
  `android-emulator-runner@v2` gerencia o boot e o encerramento. Após o ADB
  confirmar o dispositivo, o Gradle instala e executa o teste de instrumentação.

O workflow usa Python para preparar a flag no plano XCTest, interpretar os
relatórios nativos e sanitizar as últimas linhas dos logs em caso de falha.
A execução fica a cargo de Cargo, Xcode e Gradle.

## Chave e diagnósticos

Configure a SDK Project Key de teste em `secrets.QONVERSION_PROJECT_KEY`, no
Environment `test`. O workflow a fornece na compilação Rust e na sanitização
dos logs de falha. Os builds, relatórios e logs brutos ficam em um diretório
temporário privado por job. O workflow dispensa upload de artefatos e cache de
builds com a chave incorporada.

## Verificação local e transição

```sh
cargo test --locked --offline --lib -j 1
python -B -m unittest discover -s integration-tests/native-remote-config -p 'test_*.py' -v
```

Esses comandos verificam a biblioteca e os auxiliares. A consulta real precisa
passar nos dois jobs do GitHub Actions.

Durante a transição, o modo anterior `run.py ios --run` ou
`run.py android --run` ainda aceita `--device` para recuperação com um dispositivo
já iniciado e a chave fornecida externamente. Os arquivos `ci.py` e `test_ci.py`
ficam disponíveis para comparação até os dois jobs aprovarem a nova execução
nativa. A retirada deles ocorre após essa validação.
