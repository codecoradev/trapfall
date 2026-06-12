# SDK Integration

TrapFall is compatible with the Sentry envelope protocol. Any Sentry SDK can send errors to TrapFall by swapping the DSN.

## How It Works

1. Sentry SDK captures an error in your app
2. SDK sends it as an envelope to the DSN host
3. TrapFall parses the envelope, fingerprints the error
4. Error appears in the dashboard in real-time

## Supported SDKs

### Rust

```rust
// Cargo.toml: sentry = "0.35"

fn main() {
    let _guard = sentry::init((
        "https://<key>@your-server:3000/<project_id>",
        sentry::ClientOptions {
            release: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        },
    ));

    // Errors are captured automatically
    panic!("Something went wrong");
}
```

### Python

```python
# pip install sentry-sdk

import sentry_sdk

sentry_sdk.init(
    dsn="https://<key>@your-server:3000/<project_id>",
    traces_sample_rate=0.0,  # TrapFall is error-only, no tracing
)

# Capture manually
try:
    1 / 0
except Exception:
    sentry_sdk.capture_exception()
```

### JavaScript / Node.js

```js
// npm install @sentry/node

const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "https://<key>@your-server:3000/<project_id>",
});

// Capture manually
try {
  throw new Error("Something broke");
} catch (e) {
  Sentry.captureException(e);
}
```

### Browser (React, Vue, etc.)

```js
// npm install @sentry/browser

Sentry.init({
  dsn: "https://<key>@your-server:3000/<project_id>",
});
```

### Flutter / Dart

```dart
// pubspec.yaml: sentry_flutter: ^8.0.0

await SentryFlutter.init((options) {
  options.dsn = 'https://<key>@your-server:3000/<project_id>';
});

// Capture manually
try {
  throw Exception('Something went wrong');
} catch (e, stackTrace) {
  await Sentry.captureException(e, stackTrace: stackTrace);
}
```

## Limitations

TrapFall is **error capture only**. The following Sentry features are **not supported**:

- ❌ Performance / APM / tracing
- ❌ Session replay
- ❌ Log aggregation
- ❌ Distributed tracing
- ❌ OpenTelemetry / OTLP
- ❌ Profiling

Set `traces_sample_rate: 0` in your SDK config to avoid unnecessary data.
