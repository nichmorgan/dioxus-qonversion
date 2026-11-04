# Manual iOS Remote Config diagnostic

This reuses the Firebase demo's Maestro-on-GitHub-Actions pattern. It does not use Maestro Cloud, REST calls, Firebase emulators, purchases, or production Qonversion settings.

## Before the first execution

- Authorize the first run separately. No pipeline is started by preparing these files.
- Store only the **SDK Project Key for Jusfine - Rust Tests** as the Actions secret `QONVERSION_PROJECT_KEY`. Supply it through the team's approved secret-management process. Do not use a Qonversion administrative Secret Key or MCP token.
- The GitHub default branch must register `workflow_dispatch` in `.github/workflows/test.yml` before GitHub will offer manual dispatch. Adding it only to this feature branch is not sufficient. Arrange that minimal registration with the repository owner; this change does not merge or push anything.
- Check that the checkout includes `ios/Package.swift` and `src/native/ios_plugin.rs` from Stage 12.

## Manual invocation, only after authorization

Actions -> **Test** -> **Run workflow** -> branch `test/qonversion-remote-config-ios` -> enable **run_ios_remote_config**. Its default is false. The native job only runs for `workflow_dispatch` with this boolean enabled. The existing Android and iOS unit jobs remain PR-only.

Equivalent CLI command, not executed by this setup:

```text
gh workflow run test.yml --ref test/qonversion-remote-config-ios -f run_ios_remote_config=true
```

Ordinary pushes to this feature branch do not directly match any existing push trigger. If it already has an open PR, a push can emit `pull_request.synchronize`, running existing unit checks and Release Drafter. It cannot run the manual native job. Release Drafter also runs on pushes to main. The unrelated Cargo Publish workflow runs on release publication or its own manual dispatch; do not select that workflow for this test.

## Build and Bundle ID

The job uses macos-latest, Xcode/Swift already installed on the runner, Rust, Python 3.12, Java 21, Dioxus CLI 0.7.10, and Maestro CLI. It selects the Rust simulator target from the runner's CPU architecture. No iOS commands are run during local Windows validation.

The build helper temporarily copies this directory's `Dioxus.toml` to the repository root and restores/removes it in a finally block. Only this CI iOS build receives the test Bundle ID `com.example.qonversion.remoteconfig`, which matches `.maestro/01-remote-config.yaml`. The built app's actual Info.plist is checked before installation. Local and Android configuration are unchanged.

`QONVERSION_PROJECT_KEY` is available to dx at compile time, matching the example's `option_env!`. Cargo.lock is intentionally ignored by this library; the CI build follows the Firebase pattern and allows Cargo to resolve it on the fresh runner. The Qonversion Swift SDK is pinned by ios/Package.swift. This stage does not introduce a dependency cache or upload the resulting app.

## Test and evidence

Maestro uses iOS visible-text selectors from the real demo. It waits for initialization, taps Consultar Remote Config, waits for completion, and requires the strict validation success message plus all three expected values in the displayed JSON. It scrolls when necessary. Wrong JSON types fail the Rust demo validator even when the textual value looks similar. Missing host, timeout, empty payload and incorrect data cannot satisfy the success assertion.

Results are under the Actions run's artifact `ios-remote-config-<run-id>-<attempt>` (7-day retention), on success and failure:

- Sanitized build, installation, Maestro console, native stream/history and available app crash reports.
- Framework/architecture/dependency diagnostics and Rust callback-symbol observations.
- Maestro JUnit report, screenshots and diagnostic files, plus a final simulator screenshot when available.
- Collection warnings when the build failed before a usable app/simulator existed.

The helper sanitizes text before it reaches console or upload files. Only allow-listed text diagnostics and PNG screenshots from Maestro's isolated output directory are copied. Screenshots depict the demo, whose UI redacts the Project Key; literal key bytes in a PNG are rejected. Do not expand uploads to target/, raw runner directories, compiler env captures, .app bundles or archives: those may contain the compiled Project Key. GitHub console masking alone does not sanitize uploaded artifacts. Native-tool subprocesses other than dx do not receive the key environment variable.

## Remaining macOS validation

The first run must establish Swift compilation, SDK resources/dependencies, callback-symbol resolution, framework loading, simulator accessibility and the real SDK response. nm diagnostics do not by themselves prove dynamic symbol visibility. Unresolved @_silgen_name callbacks may prevent Swift linking or dlopen; this setup collects that failure without modifying native callbacks or adding speculative linker flags.

Local helper tests (stdlib only):

```text
python -B -m unittest discover -s .github/scripts -p test_ios_remote_config.py -v
```
