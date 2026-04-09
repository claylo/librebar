## Review: 2026-04-08-full-crate

| Finding | Verdict | Notes |
|---------|---------|-------|
| [config-loader-discards-discovered-config-errors](index.md#discovered-config-parse-failures-are-silently-ignored) | confirmed | — |
| [builder-ignores-configured-log-directory](index.md#loaded-config-never-reaches-logging-target-selection) | confirmed | — |
| [tracing-init-panics-instead-of-returning-error](index.md#logging-startup-panics-when-tracing-was-initialized-earlier) | confirmed | — |
| [shutdown-startup-assumes-a-tokio-runtime](index.md#shutdown-startup-assumes-an-active-tokio-runtime) | confirmed | — |
| [shutdown-token-treats-channel-close-as-cancelled](index.md#shutdowntokencancelled-returns-on-sender-drop-without-a-shutdown-signal) | confirmed | — |
| [unused-anyhow-direct-dependency](index.md#the-cli-feature-carries-an-unused-anyhow-dependency) | confirmed | — |
