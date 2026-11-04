# iOS Remote Config diagnostic

This reuses the Firebase demo's Maestro-on-GitHub-Actions pattern. It does not use Maestro Cloud, REST calls, Firebase emulators, purchases, or production Qonversion settings.

## Before the first execution

- Store only the **SDK Project Key for Jusfine - Rust Tests** as the GitHub Environment **test** secret `QONVERSION_PROJECT_KEY`. Supply it through the team's approved secret-management process. Do not use a Qonversion administrative Secret Key or MCP token.
- Ensure Environment `test` allows deployments from `test/qonversion-remote-config-ios`; any required reviewers must approve the job before it starts.
- Check that the checkout includes `ios/Package.swift` and `src/native/ios_plugin.rs` from Stage 12.
- Preparing these files does not push changes or start a pipeline.

## Branch push trigger

The separate `.github/workflows/ios-remote-config.yml` workflow, named **iOS Remote Config**, runs on pushes exclusively to `test/qonversion-remote-config-ios`. The job uses Environment `test` and reads the key only through `secrets.QONVERSION_PROJECT_KEY`. There is no manual-dispatch or pull-request trigger in this workflow, and no registration on main is required for its push trigger.

A future push to that branch will trigger this test, subject to Environment protection rules. The standard `.github/workflows/test.yml` is restored exactly from the local `origin/main` reference and continues to run its original Android and iOS unit tests on pull requests. If the branch has an open PR, a push can also emit `pull_request.synchronize`, running those standard checks and Release Drafter. Release Drafter also runs on pushes to main. The unrelated Cargo Publish workflow runs on release publication or its own manual dispatch.

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

The next run must establish Swift compilation, SDK resources/dependencies, callback registration, framework loading, simulator accessibility and the real SDK response. Callbacks are registered as C function pointers through Objective-C before SDK initialization; the framework no longer uses @_silgen_name imports of Rust symbols. Existing nm diagnostics are informational: missing dynamic exports or absent callback imports are not errors for this registration strategy. No workflow changes or permissive linker flags are required by this fix.

Local helper tests (stdlib only):

```text
python -B -m unittest discover -s .github/scripts -p test_ios_remote_config.py -v
```
