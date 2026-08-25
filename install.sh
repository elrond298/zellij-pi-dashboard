#!/bin/sh
set -eu

repo=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
pi_home=${PI_AGENT_DIR:-"$HOME/.pi/agent"}
config="$config_home/zellij/config.kdl"
plugin="$data_home/zellij/plugins/zellij-pi-dashboard.wasm"
extension="$pi_home/extensions/zellij-status.ts"
marker="// zellij-pi-dashboard"

command -v cargo >/dev/null
command -v zellij >/dev/null

cargo build --manifest-path "$repo/Cargo.toml" --release --target wasm32-wasip1
install -Dm644 "$repo/target/wasm32-wasip1/release/zellij-pi-dashboard.wasm" "$plugin"
mkdir -p "$(dirname -- "$extension")"
ln -sfn "$repo/extensions/zellij-status.ts" "$extension"

mkdir -p "$(dirname -- "$config")"
touch "$config"
if ! grep -qF "$marker" "$config" && ! grep -qF "zellij-pi-dashboard.wasm" "$config"; then
    escaped_plugin=$(printf '%s' "$plugin" | sed 's/\\/\\\\/g; s/"/\\"/g')
    candidate=$(mktemp "${config}.tmp.XXXXXX")
    trap 'rm -f "$candidate"' EXIT HUP INT TERM
    cat "$config" >"$candidate"
    cat >>"$candidate" <<EOF

$marker
keybinds {
    session {
        bind "P" {
            LaunchOrFocusPlugin "file:$escaped_plugin" {
                floating true
                move_to_focused_tab true
            }
            SwitchToMode "normal"
        }
    }
}
EOF
    ZELLIJ_CONFIG_FILE="$candidate" zellij setup --check >/dev/null
    mv "$candidate" "$config"
    trap - EXIT HUP INT TERM
fi

printf 'Installed dashboard: %s\n' "$plugin"
printf 'Installed Pi extension: %s -> %s\n' "$extension" "$repo/extensions/zellij-status.ts"
printf 'Restart Pi and Zellij, then press Ctrl-o followed by uppercase P.\n'
