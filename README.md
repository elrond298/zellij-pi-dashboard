# Zellij Pi Dashboard

A Zellij plugin that aggregates Pi processes in the current Zellij session and shows their goals, todos, progress, active tools, agents, elapsed/busy time, and token usage.

## Install

```sh
./install.sh
```

The installer builds the WASM plugin, installs it under `${XDG_DATA_HOME:-~/.local/share}/zellij/plugins/`, links the Pi extension, and adds the `Ctrl o` then uppercase `P` binding when one does not already exist. Restart Pi and Zellij after installation.

## Install the Pi status extension

The tracked extension is [`extensions/zellij-status.ts`](extensions/zellij-status.ts). Link it into Pi:

```sh
ln -sfn "$PWD/extensions/zellij-status.ts" ~/.pi/agent/extensions/zellij-status.ts
```

Restart running Pi processes after changing the extension.

## Build and open

```sh
cargo build --release --target wasm32-wasip1
zellij plugin --floating --x 5% --y 10% --width 90% --height 80% -- \
  "file:$PWD/target/wasm32-wasip1/release/zellij-pi-dashboard.wasm"
```

## Key binding

Add this inside the `session { ... }` block under `keybinds` in `~/.config/zellij/config.kdl`. Replace the plugin path with the absolute path to this repository:

```kdl
bind "P" {
    LaunchOrFocusPlugin "file:/absolute/path/to/zellij-pi-dashboard/target/wasm32-wasip1/release/zellij-pi-dashboard.wasm" {
        floating true
        move_to_focused_tab true
    }
    SwitchToMode "normal"
}
```

The active Zellij config binds `Ctrl o`, then uppercase `P`, to open or focus this floating dashboard. Press `Esc` inside it to close it.

On first launch, manually approve `RunCommands`, `ReadApplicationState`, and `ChangeApplicationState`. The plugin uses them once per second to locate and read the private JSON status files written under `${XDG_RUNTIME_DIR:-/tmp}/pi-zellij-status-$UID/`.

## Producer contract

The extension atomically writes one JSON object per Pi process to:

```text
${XDG_RUNTIME_DIR:-/tmp}/pi-zellij-status-${UID}/session-${SANITIZED_ZELLIJ_SESSION_NAME}/${PID}.json
```

`SANITIZED_ZELLIJ_SESSION_NAME` replaces characters outside `[A-Za-z0-9_.-]` with `_`. The root/session directories are mode `0700`; files are mode `0600`. Schema version 2 is:

```ts
type Status = {
  version: 2; pid: number; paneId?: string; sessionId?: string;
  sessionName?: string; instanceName?: string; cwd?: string; model?: string; thinking?: string;
  busy: boolean; tool?: string; tools: Tool[]; goal?: string; goalDetail?: Goal;
  todo?: Todo; progress?: number; subagents: string[]; agents: Agent[]; tokens?: Tokens;
  startedAt: number; busyMs: number; activityStartedAt?: number; updatedAt: number;
};
type Tool = { id: string; name: string; args?: string; startedAt: number };
type Goal = { id: string; text: string; status: string; startedAt: number; updatedAt: number; iteration: number; tokensUsed: number; timeUsedSeconds: number };
type Todo = { total: number; pending: number; active: number; completed: number; detail?: string; items: { id?: string; status: "pending" | "in_progress" | "completed"; text: string }[] };
type Agent = { id: string; type?: string; name?: string; description?: string; status: string; startedAt?: number; completedAt?: number };
type Tokens = { input: number; output: number; cacheRead: number; cacheWrite: number; total: number; cost: number; toolCalls: number };
```

`agents` lists every running agent first, followed by the three most recently ended agents.

`zjstatus/examples/pi-status.sh` consumes this layout and schema unchanged.

Use `Tab` to jump to the next Pi instance; use `j`/`k`, arrow keys, the mouse wheel, or `g` to scroll. Run `cargo test` for the parser/rendering check.
