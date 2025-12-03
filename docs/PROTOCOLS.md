# Dewet Bridge Protocols

The Dewet daemon communicates with both the Godot puppet window and the debug window through a JSON-over-WebSocket transport. The schema mirrors the data types defined in `crates/dewet-daemon/src/bridge/messages.rs`.

## Message Envelopes

- Every payload includes a `"type"` discriminator (`snake_case`).
- All timestamps are Unix seconds (`i64`).
- Binary blobs (audio, rendered textures) are base64 strings.

### Client → Daemon

| Type | Description |
|------|-------------|
| `ping` | Keep-alive with optional nonce |
| `user_chat` | Text typed by the user (`text`) |
| `optical_render_result` | Rendered PNGs for memory/chat/status quadrants (`memory`, `chat`, `status`) |
| `debug_command` | Manual controls from the debug window (adjust cooldowns, force speak, etc.) |

### Daemon → Client

| Type | Description |
|------|-------------|
| `hello` | Version + capabilities negotiated on connect |
| `speak` | Character speech instructions, including `text`, `audio_base64`, and puppet cues |
| `react` | Non-verbal reaction/emote instructions |
| `render_optical_memory` | Requests Godot to produce refreshed PNGs for the composite |
| `decision_update` | Debug broadcast describing arbiter decisions |
| `observation_snapshot` | Screen OCR summaries + metadata for the debug UI |

See `shared/schemas/bridge_protocol.json` for a machine-consumable definition.

