# Dewet Architecture Digest

This document condenses the larger `dewet-project-plan.md` into an actionable reference for implementation work. It is intentionally high-level so the source of truth stays inside the plan.

## Processes

| Process | Tech | Purpose |
|---------|------|---------|
| Dewet Daemon | Rust | Captures screen context, orchestrates LLM decisions, persists memories, serves the WebSocket bridge |
| Puppet Window | Godot 4 | Visual presentation (avatars, optical memory renders, HUDs) and bidirectional chat |
| Debug Window | Tauri + Svelte | Developer tooling: decision log, memory browser, live logs, manual controls |

## Data Flow Overview

1. **Vision** – The daemon captures the active monitor periodically (or when a diff is detected), optionally runs OCR, and marshals a 4-quadrant composite with help from the Godot renderer.
2. **Observation Buffer** – Recent frames, chat events, and notable system events are summarized into short and medium-term context windows.
3. **Director** – An LLM-native pipeline decides whether any companion should respond, which one, and in what tone. A secondary audit pass (inner monologue) reviews responses before emitting them.
4. **Bridge** – The WebSocket bridge fan-outs daemon events to Godot + debug processes and ingests user input, rendered assets, and manual overrides.
5. **Storage** – Turso/libSQL stores episodic memories, spatial contexts, character state, and arbiter decisions with optional embeddings for semantic recall.

## Key Modules (Rust)

- `bridge` – WebSocket listener, typed messages, reconnection friendly.
- `vision` – Screen capture, diff detection, composite assembly, optional OCR hooks.
- `observation` – Rolling buffers, short/medium-term summaries, event tagging.
- `storage` – Turso connection pool, CRUD for episodes/chat/character state.
- `llm` – Provider-agnostic client (LM Studio or OpenRouter) with JSON-schema completions and vision support.
- `director` – Arbiter pipeline, cooldown policy enforcement, response execution.
- `tts` – asynchronous speech synthesis abstraction (NeuTTS + fallbacks).
- `character` – CCv2 loader, lorebook ingestion, runtime state tracking.

The implementation in `crates/dewet-daemon` maps directly to this module layout and can be used as a reference while reading the plan.

