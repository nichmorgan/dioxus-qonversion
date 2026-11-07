# Remote Config test app

This app calls `remote_config_async` from Dioxus `spawn`, which is the iOS path
that used to return `MainThread` and never fetch. It expects the active Remote
Config for context key `dioxus_integration_test`:

| Field | Type | Value |
| --- | --- | --- |
| `feature_test_enabled` | boolean | `true` |
| `test_id` | string | `dioxus_ios_remote_config` |
| `message` | string | `Remote Config funcionando` |

Set `QONVERSION_PROJECT_KEY` to the SDK Project Key before building. Do not
commit the key. The iOS job in `.github/workflows/remote-config-e2e.yml` builds
this crate with `dx` and drives `maestro/`.
