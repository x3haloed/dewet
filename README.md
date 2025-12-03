# Dewet

Dewet is a desktop companion stack composed of three cooperating parts:

- **Dewet Daemon** – a Rust background service that captures screen context, reasons about when companions should speak, and brokers state between the UI layers.
- **Godot Puppet Window** – a Godot 4 project that renders characters, optical memory panels, and receives directives from the daemon.
- **Debug Window** – a desktop UI (Tauri/Svelte) for inspecting memories, decisions, and live logs.

This repository currently contains the Rust daemon, the Godot project skeleton, shared schemas, and documentation outlines derived from `dewet-project-plan.md`.

## Local Development

### One Command (Daemon + Debug + Godot)

```bash
# From the repo root
cargo run -p xtask -- dev
```

This helper ensures the `debug-ui` bundle is rebuilt, then spawns:

- `cargo run -p dewet-daemon`
- `cargo tauri dev` inside `crates/dewet-debug`
- `godot4 --path godot --scene main/Dewet.tscn`

Use flags such as `--no-godot`, `--no-debug`, or `--skip-ui-build` to tailor a session (see `cargo run -p xtask -- dev --help`).

### Manual Control

```bash
# Compile the daemon
cargo build --package dewet-daemon

# Run it with default configuration
cargo run --package dewet-daemon
```

The daemon exposes a WebSocket bridge on `ws://127.0.0.1:7777` by default. Godot and the debug window should connect to that bridge for realtime updates. To inspect live context, run the Tauri-based debug window:

```bash
# Build the web UI once
cd debug-ui && npm install && npm run build

# Launch the desktop shell
cd ../crates/dewet-debug
cargo tauri dev
```

For the puppet window, open `godot/project.godot` in Godot 4.3+ and run the `Dewet.tscn` scene. The `OpticalMemory` utility automatically listens for render requests from the daemon and streams the resulting quadrants back over the bridge.

## Repository Layout

```
.
├── Cargo.toml
├── README.md
├── crates/
│   ├── dewet-daemon/        # Rust daemon (brain)
│   └── dewet-debug/         # Tauri debug window (eyes on)
├── debug-ui/                # Vite/Svelte frontend bundled into the debug window
├── godot/                   # Godot 4 project (puppet window)
├── characters/              # Sample Character Card v2 files
├── config/                  # Dewet configuration templates
├── docs/                    # Additional markdown documentation
└── shared/                  # JSON schemas shared across processes
```

Refer to `dewet-project-plan.md` for the full architectural narrative.

