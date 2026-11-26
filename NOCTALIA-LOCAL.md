# NectarLink Nightly 13 + Noctalia (local test)

Upstream base: `v0.1.6-nightly.13`, `8eb7616c1a0de9aa7ef68cdef5532b81b37e0179`. The Noctalia palette and extended loopback API are layered on top. Wayland capture uses Nightly 13's PipeWire/OpenGL fallback, and the idle virtual camera uses the upstream cache. Linux upstream self-updates are disabled for this copy.

The API still requires a loopback connection and a bearer token, and rejects requests with an `Origin` header. For the Notifications feed and Messages section, enable **Notifications and messages** under the local automation API settings if you want the plugin to read those data. The separate switch is off by default, as in upstream.

```sh
QMAKE=/usr/bin/qmake6 cargo test --locked -p nectarlink-desktop local_api
QMAKE=/usr/bin/qmake6 cargo build --locked -p nectarlink-desktop
```

The official Android Nightly 13 APK is separate; Android sources match the upstream tag. For a first launch without the existing desktop profile, use an explicit empty `--data-dir`. The Noctalia plugin reads only the standard profile, so this isolated launch is not a plugin/paired-phone integration test. Do not replace the installed desktop build until the test copy is ready.
