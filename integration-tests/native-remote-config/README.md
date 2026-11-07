# Native Remote Config test

The Android job checks that the public Rust API can read a real Qonversion
Remote Config with no UI. It initializes the SDK in Sandbox, calls
`remote_config("dioxus_integration_test")`, and checks the returned values.

| Field | Type | Value |
| --- | --- | --- |
| `feature_test_enabled` | boolean | `true` |
| `test_id` | string | `dioxus_ios_remote_config` |
| `message` | string | `Remote Config funcionando` |

An SDK error, a timeout, an empty payload, or a missing or wrong field fails
the test.

## GitHub Actions

`.github/workflows/remote-config-e2e.yml` runs on `workflow_dispatch` and on
pushes to `main`. It uses the `test` environment secret `QONVERSION_PROJECT_KEY`
(the SDK Project Key, not a Secret Key).

The Android job builds this crate with `--features native-sdk-test`, loads it
through JNI, and runs the instrumentation test on an API 34 emulator with
`-PrunNativeRemoteConfig=true`. The test keeps a transparent activity in the
foreground, which the SDK needs before the fetch can finish. `run.py
verify-android` requires exactly one passing test and no failures or skips.

The iOS check is the separate `dx` build of
`integration-tests/remote-config-app`, driven by Maestro. That path loads the
Swift host the way an app built with the Dioxus CLI does.

## Local checks

```sh
python -B -m unittest discover -s integration-tests/native-remote-config -p 'test_*.py' -v
```

That checks the report parser. A real device run needs `QONVERSION_PROJECT_KEY`
and the Android job's Gradle invocation.
