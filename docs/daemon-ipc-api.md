# Daemon IPC API (Draft)

## Purpose
Define the daemon IPC API, split into control commands and notification events. This covers external notification plugins (any language), a socket design that supports 0-n subscribers, and multi-language triggering so plugins can react to language-specific actions.

## Goals
- Keep the core design simple and aligned with the existing Unix socket philosophy.
- Allow third-party tools in any language to subscribe to daemon events.
- Provide a small set of command endpoints for controlling language selection.
- Support multiple concurrent plugin subscribers without complex dependencies.
- Provide clear given-when-then use cases.

## Non-Goals
- No heavy RPC frameworks (gRPC/HTTP) in the daemon.
- No auth or remote network access; local user session only.
- No plugin lifecycle manager beyond socket connection/disconnection.

## Overview
Two Unix sockets are used:
- Control socket: short-lived connections for commands (existing behavior).
- Events socket: long-lived connections for event fan-out.

Events are emitted by the daemon and broadcast to all connected plugin clients. Plugins are external processes that connect to the events socket, read JSONL messages, and react accordingly.

## Socket Design

### Control Socket (existing)
- Path: `${XDG_RUNTIME_DIR}/soundvibes/sv.sock`
- Mode: Unix stream socket
- Connection: one request per connection, daemon replies with a single line response
- Payload: single-line command string
- Response: single-line JSON

### Events Socket (new)
- Path: `${XDG_RUNTIME_DIR}/soundvibes/sv-events.sock`
- Mode: Unix stream socket
- Connection: long-lived, daemon writes JSONL events
- Fan-out: daemon broadcasts each event to all connected clients
- Backpressure: if a client is slow, daemon may drop that client
- Connection handshake: none; connecting implies subscription

## Protocol

### Encoding
- UTF-8 JSON lines (JSONL)
- Each message is a single JSON object followed by `\n`
- Unknown fields must be ignored by clients

### Versioning
- Each message includes `api_version` (string, e.g. "1")
- Breaking changes bump `api_version`

## Control Commands

### Command: toggle
Toggle recording in the daemon. Optional `lang` selects a language context.

Request:
```
toggle
```

Request with language:
```
toggle lang=fr
```

Response:
```json
{"api_version":"1","ok":true,"state":"recording","language":"fr"}
```

### Command: status
Request daemon status.

Request:
```
status
```

Response:
```json
{"api_version":"1","ok":true,"state":"idle","language":"en","active_models":["en","fr"],"recording":false}
```

### Command: set-language
Set active language without toggling.

Request:
```
set-language lang=sv
```

Response:
```json
{"api_version":"1","ok":true,"language":"sv"}
```

### Command: stop
Stop the daemon.

Request:
```
stop
```

Response:
```json
{"api_version":"1","ok":true}
```

## Events

### Event: daemon_ready
Emitted when daemon is ready to accept control.

```json
{"api_version":"1","type":"daemon_ready","timestamp":"2026-02-05T12:00:00Z"}
```

### Event: recording_started
Emitted when recording starts.

```json
{"api_version":"1","type":"recording_started","timestamp":"2026-02-05T12:01:00Z","language":"fr"}
```

### Event: recording_stopped
Emitted when recording stops.

```json
{"api_version":"1","type":"recording_stopped","timestamp":"2026-02-05T12:01:10Z","language":"fr"}
```

### Event: transcript_final
Emitted when transcription completes.

```json
{"api_version":"1","type":"transcript_final","timestamp":"2026-02-05T12:01:12Z","language":"fr","utterance":1,"duration_ms":1200,"text":"bonjour"}
```

### Event: model_loaded
Emitted when a model context is loaded or activated.

```json
{"api_version":"1","type":"model_loaded","timestamp":"2026-02-05T12:01:05Z","language":"fr","model_size":"small","model_language":"fr"}
```

### Event: error
Emitted when the daemon encounters a recoverable error.

```json
{"api_version":"1","type":"error","timestamp":"2026-02-05T12:01:06Z","message":"audio device not found"}
```

## Language Handling

### Language identifiers
- Use ISO language codes (e.g. `en`, `fr`, `sv`).
- Special value `auto` is allowed for automatic detection.

### Multi-language behavior
Model pool strategy: the daemon preloads and keeps multiple language models resident. `toggle lang=...` selects which model is used for the current recording. The API always includes a `language` field in events and responses.

## Fan-out Behavior
- The daemon maintains a list of event subscribers.
- Each event is written to all connected clients.
- If a client write fails or blocks beyond a short threshold, the client is removed.

## Given-When-Then Use Cases

### Use Case: No plugins connected
Given the daemon is running
When no plugin connects to the events socket
Then recording and transcription still work normally
And no errors are logged about missing plugins

### Use Case: Single plugin receives recording events
Given the daemon is running
And a plugin connects to `sv-events.sock`
When the user toggles recording on
Then the plugin receives `recording_started`
And the event includes the selected language

### Use Case: Multiple plugins receive the same event
Given the daemon is running
And three plugins are connected to `sv-events.sock`
When a transcript completes
Then all three plugins receive the same `transcript_final` event

### Use Case: Slow plugin is dropped
Given the daemon is running
And one plugin stops reading from `sv-events.sock`
When the daemon emits events
Then the daemon disconnects the slow plugin
And continues sending events to remaining plugins

### Use Case: Plugin reconnects after daemon restart
Given a plugin is running
When the daemon is not available
Then the plugin retries connecting with backoff
When the daemon starts
Then the plugin reconnects and receives events

### Use Case: Toggle with explicit language
Given the daemon is running
When the user sends `toggle lang=fr`
Then the daemon begins recording using language `fr`
And the `recording_started` event includes `language=fr`

### Use Case: Default language for plain `sv`
Given the daemon is running
And the client has a configured default language (e.g. `en`)
When the user runs `sv` with no language specified
Then the client sends `toggle lang=en`
And the `recording_started` event includes `language=en`

### Use Case: Switch language without recording
Given the daemon is running
And recording is off
When the user sends `set-language lang=sv`
Then the daemon updates active language to `sv`
And emits a `model_loaded` or `language_changed` event

### Use Case: Plugin reacts to recording events
Given a plugin is connected
When `recording_started` is received
Then the plugin performs a start action
When `recording_stopped` is received
Then the plugin performs a stop action

## Error Handling and Compatibility
- Control responses always include `ok` boolean; error responses include `error` and `message`.
- Plugins must ignore unknown event types for forward compatibility.
- The daemon should continue operation even if plugins misbehave or disconnect.

## Open Questions
- Should `set-language` return `model_loaded` vs `language_changed` event name?
- Should the daemon ever introduce event filtering per subscriber?
