# Remote Config nativo: teste adicional em Rust

Este teste usa os SDKs reais e não apresenta telas, não inicia Dioxus/WebView e não usa Maestro. O demo e o teste ponta a ponta existente continuam intactos.

## Contrato compartilhado

`src/lib.rs` chama as APIs públicas `dioxus_qonversion::initialize()` e `remote_config("dioxus_integration_test")`. A configuração usa Sandbox e SubscriptionManagement, preservando a inicialização de Qonversion e No-Codes da biblioteca.

O resultado real precisa conter:

- `feature_test_enabled`: booleano `true`;
- `test_id`: string `dioxus_ios_remote_config`;
- `message`: string `Remote Config funcionando`.

Ausência de campos, tipos/valores incorretos e objeto vazio falham. Context Key diferente também falha quando presente em `source`. Campos adicionais não são rejeitados. Erros de inicialização, HostMissing, erros nativos, JSON malformado e timeout propagam-se pelas APIs existentes e falham no runner. A biblioteca continua aceitando `{}` como sucesso; somente este contrato de integração exige os campos.

A consulta tem timeout de 30 segundos. XCTest aguarda até 45 segundos; JUnit limita o teste a 60 segundos. O script também limita os processos externos. Timeout não cancela callbacks do SDK: não há retry no teste, e cada execução deve usar um novo processo nativo. Em timeout externo, o comando falha; pode ser necessário encerrar o aplicativo de teste ou reiniciar o emulador antes da próxima tentativa.

## Opt-in e Project Key

Sem `--run`, o script imprime `SKIPPED` e sai sem compilar, carregar SDKs ou solicitar credenciais:

```text
python integration-tests/native-remote-config/run.py ios
python integration-tests/native-remote-config/run.py android
```

A crate também possui a feature `native-sdk-test`, desabilitada por padrão. O script só a habilita com `--run`. As entradas C/JNI ficam ausentes sem essa feature. XCTest exige a variável de execução `QONVERSION_RUN_NATIVE_REMOTE_CONFIG=1`, inserida pelo script no plano gerado. JUnit exige o argumento `runNativeRemoteConfig=true`, fornecido pelo script ao Gradle. Um teste ignorado nunca conta como integração aprovada: o runner exige exatamente um teste aprovado, zero falhas e zero skips nos relatórios.

Forneça `QONVERSION_PROJECT_KEY` externamente antes do build: somente a SDK Project Key de **Jusfine - Rust Tests**, nunca Secret Key administrativa ou token MCP. `option_env!` recebe a chave na compilação Rust. Não há chave literal nem arquivo `.env` no projeto. Somente o subprocesso Cargo recebe essa variável; argumentos de comandos, planos XCTest e configuração JUnit não contêm a chave. Console e erros Rust substituem ocorrências da chave por `[REDACTED]`.

A chave fica incorporada no binário e nos metadados locais do Cargo. Mantivemos `option_env!` e o fornecimento externo: a correção protege a exportação dos artefatos, sem mudar os hosts ou a integração. Cada execução habilitada cria `artifacts/run-*/raw/` para arquivos sensíveis e `artifacts/run-*/safe/` para os dois diagnósticos textuais permitidos. Nunca publique a pasta da execução inteira, binários, bibliotecas, APKs, caches ou relatórios nativos brutos. O diretório de artefatos é ignorado pelo Git. O Cargo.toml e o Cargo.lock da biblioteca original permanecem intactos.

Em uma futura execução no GitHub Actions, o job deve declarar `environment: test` e fornecer a variável ao passo de execução exclusivamente por `${{ secrets.QONVERSION_PROJECT_KEY }}`. Os runners locais não acessam secrets do GitHub nem executam pipelines; o novo workflow apenas prepara sua execução futura. Configure a SDK Project Key no Environment `test` pelo processo de gestão de segredos da equipe.

## Contrato de automacao e artefatos

| Estado | Codigo de saida | Significado |
|---|---:|---|
| PASSED | 0 | O runner nativo confirmou exatamente um teste aprovado. |
| FAILED | 1 | Erro de configuracao, build, SDK, validacao ou seguranca dos artefatos. |
| SKIPPED | 77 | Ausencia de `--run`: nenhuma ferramenta nativa ou build foi iniciado. |

O runner imprime uma linha `NATIVE_TEST_RESULT {"status": ..., "exit_code": ...}`. Erros de argumentos tambem produzem FAILED/1. `--help` apenas mostra a ajuda e nao representa uma execucao. Em CI, invoque com `--run`; nao converta automaticamente codigo 77 em aprovacao e nao use `continue-on-error` para transformar uma falha em sucesso.

A pasta por execucao contem:

```text
artifacts/run-<id>/
  raw/                 # PRIVADO: nunca publicar nem armazenar em cache
    target/            # binarios e fingerprints Cargo com a chave incorporada
    tmp/               # temporarios de compiladores/subprocessos
    logs/              # stdout/stderr original, potencialmente sensivel
    ios/DerivedData/   # app, testes e xcresult brutos
    jniLibs/           # copia da biblioteca JNI usada no APK
    gradle-build/      # builds de app e do host existente; APKs/XML/HTML
    gradle-user-home/  # cache Gradle isolado
    gradle-project-cache/
  safe/                # UNICO diretorio elegivel para upload
    diagnostics.txt    # texto sanitizado; nenhum relatorio bruto copiado
    result.json        # estado, codigo e diagnostico sanitizado
```

`command()` captura a saida original em `raw/logs/` e sanitiza console e `safe/diagnostics.txt`, incluindo saidas parciais de timeout. A sanitizacao remove escapes ANSI e mascara a chave literal e representacoes comuns JSON, URL e Base64. Apenas Cargo recebe a variavel da chave; os outros subprocessos recebem o binario ja compilado. No macOS/Linux os novos diretorios privados usam modo 0700; isso nao substitui o isolamento e as permissoes do runner.

Antes de disponibilizar `safe_artifacts` no resultado JSON, o runner verifica a lista exata de nomes permitidos, UTF-8, ausencia de NUL, links simbolicos e ocorrencias conhecidas da chave. Se a verificacao falhar, retorna FAILED e nao fornece caminho para upload. Nao existe copia generica de raw/ para safe/ nem comando de publicacao. A chave incorporada continua existindo nos arquivos privados: nao use upload por glob amplo, nao arquive o workspace e nao reutilize caches desses builds.

Um futuro workflow separado deve usar `environment: test`, fornecer a chave somente por `secrets.QONVERSION_PROJECT_KEY` no passo que chama este runner e coletar exclusivamente o diretorio `safe_artifacts` validado daquela execucao, inclusive em falhas. Nao envie raw/, DerivedData, target/, jniLibs/, APKs, SOs, XMLs ou xcresults. Os workflows preexistentes permanecem intactos. O workflow independente descrito abaixo foi adicionado na etapa seguinte.

## iOS primeiro

Requisitos: macOS, Xcode 16 ou posterior selecionado por `xcode-select`, runtime iOS Simulator instalado, Python 3, Rust com o target correto e XcodeGen 2.46 ou compatível. XcodeGen evita manter manualmente um `.pbxproj`; o projeto gerado é ignorado. Instale a ferramenta pelo processo da equipe (por exemplo, Homebrew `brew install xcodegen`).

```sh
# Instalar o target correspondente ao runner; o script seleciona pela CPU do Mac.
rustup target add aarch64-apple-ios-sim  # Apple Silicon
# Em Mac Intel: rustup target add x86_64-apple-ios
xcrun simctl list devices available
# Defina IOS_SIMULATOR_UDID com o UDID escolhido e inicialize-o se necessário.
xcrun simctl boot "$IOS_SIMULATOR_UDID"
xcrun simctl bootstatus "$IOS_SIMULATOR_UDID" -b
# QONVERSION_PROJECT_KEY deve estar fornecida externamente, sem echo.
python3 integration-tests/native-remote-config/run.py ios --run --device "$IOS_SIMULATOR_UDID"
```

O script compila a biblioteca Rust e obtém os requisitos de link do próprio `rustc --print native-static-libs`, sem adivinhar flags. Gera o projeto Xcode, executa `build-for-testing`, habilita o teste no `.xctestrun`, executa `test-without-building` e restringe a execução a `NativeRemoteConfigTests/RemoteConfigTests/testRealRemoteConfig` e verifica o resumo via `xcresulttool get test-results summary`. Exige contadores inteiros de exatamente um teste total/aprovado, zero falhas e zero skips; rejeita expected failures, detalhes de falhas e estado de falha quando declarados. JSON ausente, inválido ou inconsistente não aprova a integração.

A interface `get test-results` está disponível no Xcode 16; consulte a [documentação Apple do xcresulttool](https://developer.apple.com/documentation/xcode-release-notes/xcode-16_3-release-notes) e `xcrun xcresulttool help get test-results summary` no Mac para conferir o schema instalado.

O aplicativo hospedeiro contém somente `UIApplicationDelegate`, sem janela ou controller. Vincula o pacote Swift **existente** em `../../../ios`, com Qonversion 6.13.0 e seus recursos. Uma referência concreta à classe mantém o host no aplicativo. XCTest chama a inicialização Rust no main actor; a consulta Rust roda em thread de trabalho enquanto XCTest aguarda assincronamente. O caminho é:

`XCTest → entrada C de teste → initialize/remote_config Rust → Objective-C → host Swift existente → SDK Qonversion`.

As strings de erro retornadas por Rust são copiadas e liberadas explicitamente. Panics não atravessam a ABI. O processo mantém o bundle de testes carregado enquanto callbacks existirem.

Este cenário confirma a ponte Rust/Swift com o host previamente vinculado. **Não valida o empacotamento Manganis nem o carregamento dinâmico pelo Dioxus CLI**; o teste Dioxus/Maestro continua necessário para isso. O funcionamento de Qonversion/No-Codes sem janela apresentada deve ser confirmado no Simulator, não presumido.

Resultados iOS brutos: `artifacts/run-*/raw/ios/DerivedData/NativeRemoteConfig-*.xcresult`. Somente `safe/diagnostics.txt` e `safe/result.json` da execução concluída podem ser publicados; não há upload automático.

Referência da configuração XcodeGen: https://yonaskolb.github.io/XcodeGen/Docs/ProjectSpec.html

## Android: o mesmo contrato Rust

Requisitos: JDK 17, Gradle 8.6 no PATH (ou `GRADLE` apontando para o executável), Android SDK 34/build-tools, NDK e Emulator/dispositivo com internet. O projeto reutiliza AGP 8.4.2, Kotlin 1.9.24 e o módulo `android/` original, inclusive sua dependência No-Codes, sem copiar o host ou alterar a biblioteca. Não adicionamos outro Gradle Wrapper.

Defina externamente `ANDROID_HOME`, `ANDROID_NDK_HOME` e `JAVA_HOME`. No Windows, `GRADLE` pode apontar para `gradle.bat`. Instale o target correspondente:

```text
rustup target add aarch64-linux-android
rustup target add x86_64-linux-android
adb devices
python integration-tests/native-remote-config/run.py android --run --device emulator-5554
```

Use somente o dispositivo pretendido conectado durante essa execução. O script consulta sua ABI, configura o linker do NDK para API 24, compila a biblioteca JNI, copia o `.so` para o aplicativo de teste e executa `:app:connectedDebugAndroidTest`. ABIs suportadas: arm64-v8a e x86_64.

O aplicativo não declara Activity. AndroidJUnitRunner fornece o contexto da aplicação e o main looper. `RustBridge` carrega a biblioteca no ClassLoader da aplicação alvo. O adaptador JNI de teste inicializa `ndk_context` uma única vez com JavaVM e uma referência global ao contexto, mantida até o fim do processo para não invalidar workers/callbacks. A thread de instrumentação/JUnit chama o contrato Rust; o host Kotlin existente agenda suas operações no main looper:

`JUnit → RustBridge/JNI → initialize/remote_config Rust → JNI → host Kotlin existente → SDK Qonversion`.

As validações do payload ficam integralmente em Rust. Os testes Swift/Kotlin não chamam o SDK diretamente. Os resultados JUnit ficam em `raw/gradle-build/app/outputs/androidTest-results/connected/` e os relatórios HTML em `raw/gradle-build/app/reports/androidTests/connected/`, sob a pasta da execução. O runner rejeita resultados ausentes, antigos, inconsistentes, falhas, skips e estados `status`/`result` não executados. Atributos ausentes são compatíveis com o XML padrão do Gradle; se declarados, `status` aceita somente `run`/`passed` e `result` aceita somente `completed`/`passed`.

## Verificações locais e limites

```text
cargo check --offline --manifest-path integration-tests/native-remote-config/Cargo.toml -j 1
cargo check --offline --manifest-path integration-tests/native-remote-config/Cargo.toml --features native-sdk-test --target aarch64-linux-android -j 1
cargo check --offline --manifest-path integration-tests/native-remote-config/Cargo.toml --features native-sdk-test --target x86_64-linux-android -j 1
cargo fmt --manifest-path integration-tests/native-remote-config/Cargo.toml --check
python -B -m unittest discover -s integration-tests/native-remote-config -p test_runner.py -v
cargo test --locked --offline --lib -j 1
```

Depois de gerar o lockfile local da crate adicional, pode-se acrescentar `--locked` aos seus checks. Os testes Python verificam opt-in e interpretação de relatórios; suas fixtures são arquivos de resultado, não mocks do SDK nem respostas simuladas de Remote Config.

Verificado no Windows em 04/11/2026:

- Check da crate adicional no host; checks ARM64 e x86_64 Android com a feature nativa.
- Compilação e linkedição Rust com NDK 29/API 24 para ambas as ABIs, gerando `.rlib`, `.a` e `.so`, sem Project Key. Isso não compila Kotlin nem empacota APKs.
- 18 testes Python aprovados, zero falhas: opt-in, contrato de status, interpretação de relatórios, sanitização, bloqueio de artefatos e integridade UTF-8. Os 66 testes Rust existentes não são evidência de consulta real à Qonversion.
- `cargo fmt --check`, análise sintática Python/TOML/XML e revisão da configuração XcodeGen, bridging header, caminhos dos hosts nativos e guardas de execução.

Para reproduzir a linkedição Android sem executar o SDK, mantenha `QONVERSION_PROJECT_KEY` ausente do ambiente desses comandos diretos (sem passar pelo isolamento do runner), configure `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER` (ou `CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER`) com o caminho absoluto de `ANDROID_NDK_HOME/toolchains/llvm/prebuilt/<host>/bin/<target>24-clang` (`.cmd` no Windows) e execute:

```text
cargo rustc --locked --offline --manifest-path integration-tests/native-remote-config/Cargo.toml --lib --features native-sdk-test --target aarch64-linux-android -- --print native-static-libs
cargo rustc --locked --offline --manifest-path integration-tests/native-remote-config/Cargo.toml --lib --features native-sdk-test --target x86_64-linux-android -- --print native-static-libs
```

Os artefatos dessa verificação foram compilados **sem chave** e não servem para aprovar o contrato. Para testar o SDK, use `run.py --run` com a chave externa; o script recompila com ela e exige os relatórios nativos.

Ainda pendentes: geração/compilação do projeto Xcode, vínculo Swift/Rust/SDK/recursos, execução XCTest sem janela, build Gradle/Kotlin/APKs, execução da instrumentação e retorno real do contexto nas duas plataformas. Xcode/XcodeGen/Swift não estão disponíveis neste Windows; os targets Rust iOS não estão instalados. O SDK Android 34, NDK 29 e targets Rust Android estão instalados, mas não há Gradle disponível e `adb devices` não lista dispositivos conectados. Não foram iniciados emuladores nem fornecidas credenciais. A compilação Android emite dois avisos preexistentes de depreciação em macros JNI da biblioteca; nenhum host de produção foi alterado para suprimi-los.

As APIs públicas, hosts de produção, demo, Maestro e workflows existentes permanecem inalterados. Nenhum commit, push ou PR é criado por este procedimento.

## Arquivos adicionados

Todos ficam nesta pasta; nenhum arquivo rastreado preexistente foi modificado.

```text
.gitignore
Cargo.toml
README.md
ci.py
android/app/build.gradle.kts
android/app/src/androidTest/java/io/dioxus/qonversion/nativetest/RemoteConfigTest.kt
android/app/src/main/AndroidManifest.xml
android/app/src/main/java/io/dioxus/qonversion/nativetest/RustBridge.kt
android/build.gradle.kts
android/gradle.properties
android/settings.gradle.kts
ios/AppDelegate.swift
ios/RemoteConfigTests.swift
ios/TestBridge.h
ios/project.yml
run.py
src/android.rs
src/ios.rs
src/lib.rs
test_runner.py
test_ci.py
```

## Workflow nativo independente

Arquivo: `.github/workflows/native-remote-config.yml`. O gatilho e push exclusivamente para `test/qonversion-remote-config-ios`, sem main, pull_request ou workflow_dispatch. Cada job repete a verificacao da branch/evento e usa o Environment `test`. Configure nesse Environment a SDK Project Key por `secrets.QONVERSION_PROJECT_KEY` e restrinja suas deployment branches a essa branch; a politica remota do Environment nao e configurada pelo YAML nem foi alterada localmente.

Os jobs sao independentes: uma falha iOS nao impede a tentativa Android. Nao instalam Dioxus CLI nem Maestro. Os workflows anteriores nao foram modificados; um futuro push nessa branch tambem pode acionar o workflow Dioxus/Maestro ja existente.

- iOS: macos-15, Xcode 16.4 selecionado via DEVELOPER_DIR, XcodeGen >= 2.46, Python 3.12 e Rust stable com target escolhido pela CPU do Mac. Inicializa um iPhone iOS 18.5 disponivel, chama `ci.py ios --device <UDID>` e encerra o Simulator ao final.
- Android: ubuntu-24.04 com KVM, JDK 17, Gradle 8.6 sem cache, SDK/build-tools 34, NDK 29.0.14206865 e Emulator API 34 google_apis x86_64. Inicializa o Emulator com `-no-window`, chama `ci.py android --device emulator-5554` e encerra o Emulator ao final. Os pacotes Android sao instalados explicitamente pelo sdkmanager.

`ci.py` e apenas um adaptador de processo/protocolo/publicacao: chama obrigatoriamente o `run.py` existente com `--run`. Nao implementa consulta, inicializacao de SDK ou validacao de payload. O Rust compartilhado e as pontes nativas continuam sendo executados pelo XCTest/JUnit existentes.

O adaptador exige um unico resultado final, correspondencia entre codigo de saida e status, correspondencia exata com `safe/result.json`, e os dois arquivos sanitizados obrigatorios. PASSED/0 e a unica aprovacao; FAILED/1 permanece falha mesmo com diagnosticos coletados; SKIPPED/77 e sempre rejeitado pelo CI. Relatorio ausente, incoerente ou inseguro falha. Os parsers nativos do run.py continuam exigindo exatamente um teste real aprovado e validacao estrita em Rust.

Antes de autorizar upload, verifica que o caminho esta exatamente em `artifacts/run-*/safe`, sem traversal, fora da raiz ou links simbolicos nos diretorios/arquivos. Reutiliza `Artifacts.validate_publication` para validar nomes, UTF-8 e ausencia da chave conhecida. Copia somente `diagnostics.txt` e `result.json` para `RUNNER_TEMP/native-remote-config-publish/<plataforma>/`; nenhum arquivo bruto e copiado. O caminho so e escrito em GITHUB_OUTPUT depois das verificacoes. O upload usa exclusivamente esse output, com `always()` para preservar diagnosticos autorizados em falhas e retencao de 7 dias. Ausencia de arquivos no upload autorizado e erro.

A chave existe somente no ambiente do passo que executa ci.py; nao e fornecida a instaladores nem ao boot dos emuladores. Nao ha cache Cargo ou Gradle no workflow. Logs privados de boot do Emulator ficam no RUNNER_TEMP e nao sao publicados.

Regressoes locais (fixtures de protocolo/relatorios, nunca mocks de SDK):

```text
python -B -m unittest discover -s integration-tests/native-remote-config -p "test_*.py" -v
```

Inclui os 18 testes anteriores e os testes de `test_ci.py`: preservacao de falha, rejeicao de skip/codigos incoerentes, caminhos externos/raw/traversal, relatorios ausentes, arquivos extras, texto nao sanitizado e divergencia entre console e JSON. Esses testes nao aprovam uma consulta real.

Referencias oficiais usadas para conferir a infraestrutura:

- [Imagem macOS 15 e Xcode disponivel](https://github.com/actions/runner-images/blob/main/images/macos/macos-15-Readme.md).
- [Parametros setup-gradle v4, incluindo cache-disabled](https://github.com/gradle/actions/blob/v4/setup-gradle/action.yml).
- [Padrao de KVM e Emulator em runners Linux](https://github.com/ReactiveCircus/android-emulator-runner).

A disponibilidade exata de downloads, o build Swift/XCTest e Gradle/Kotlin, o ciclo dos emuladores e o retorno real da Qonversion precisam ser confirmados na primeira execucao autorizada do GitHub Actions. Nenhum pipeline foi executado para preparar esta configuracao.
