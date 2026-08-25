# Zellij Pi Dashboard

A Zellij plugin that aggregates Pi processes in the current Zellij session and shows their goals, todos, progress, active tools, agents, elapsed/busy time, and token usage.

## Install the Pi status extension

The tracked extension is [`extensions/zellij-status.ts`](extensions/zellij-status.ts). Link it into Pi:

```sh
ln -sfn "$PWD/extensions/zellij-status.ts" ~/.pi/agent/extensions/zellij-status.ts
```

Restart running Pi processes after changing the extension.

## Build and open

```sh
cargo build --release --target wasm32-wasip1
zellij action start-or-reload-plugin "file:$PWD/target/wasm32-wasip1/release/zellij-pi-dashboard.wasm"
```

On first launch, manually approve `RunCommands` and `ReadApplicationState`. The plugin uses them once per second to locate and read the private JSON status files written under `${XDG_RUNTIME_DIR:-/tmp}/pi-zellij-status-$UID/`.

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

`zjstatus/examples/pi-status.sh` consumes this layout and schema unchanged.

Use `j`/`k`, arrow keys, or `g` to scroll. Run `cargo test` for the parser/rendering check.
