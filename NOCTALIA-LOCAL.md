# NectarLink Nightly 12 + Noctalia (local test)

Upstream base: `v0.1.6-nightly.12`, `7e1495d644520dab4745a9268f077e2c7f7d4899`. Noctalia source: `0e60281` of `nectarlink_noctalia`. The upstream Niri capture, keyboard and Island changes are kept. The Noctalia palette and extended loopback API are layered on top. Linux upstream self-updates are disabled for this copy.

The API still requires a loopback connection and a bearer token, and rejects requests with an `Origin` header. For the Notifications feed and Messages section, enable **Notifications and messages** under the local automation API settings if you want the plugin to read those data. The separate switch is off by default, as in upstream.

```sh
QMAKE=/usr/bin/qmake6 cargo test --locked -p nectarlink-desktop local_api
QMAKE=/usr/bin/qmake6 cargo build --locked -p nectarlink-desktop
```

The upstream Android Nightly 12 APK is separate and unchanged. For a first launch without the existing desktop profile, use an explicit empty `--data-dir`. The Noctalia plugin reads only the standard profile, so this isolated launch is not a plugin/paired-phone integration test. Do not replace the installed desktop build until the test copy is ready.
