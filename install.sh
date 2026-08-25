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
base=$(mktemp "${config}.base.XXXXXX")
candidate=$(mktemp "${config}.tmp.XXXXXX")
trap 'rm -f "$base" "$candidate"' EXIT HUP INT TERM

# Migrate the block appended by older installers; a second keybinds block is ignored.
sed '/^\/\/ zellij-pi-dashboard$/,$d' "$config" >"$base"
if grep -qF "zellij-pi-dashboard.wasm" "$base"; then
    cat "$base" >"$candidate"
else
    escaped_plugin=$(printf '%s' "$plugin" | sed 's/\\/\\\\/g; s/"/\\"/g')
    if grep -Eq '^[[:space:]]*session[[:space:]]*\{[[:space:]]*$' "$base"; then
        awk -v plugin="$escaped_plugin" -v marker="$marker" '
            {
                print
                if (!inserted && $0 ~ /^[[:space:]]*session[[:space:]]*\{[[:space:]]*$/) {
                    indent = $0
                    sub(/session[[:space:]]*\{[[:space:]]*$/, "", indent)
                    indent = indent "    "
                    print indent marker
                    print indent "bind \"P\" {"
                    print indent "    LaunchOrFocusPlugin \"file:" plugin "\" {"
                    print indent "        floating true"
                    print indent "        move_to_focused_tab true"
                    print indent "    }"
                    print indent "    SwitchToMode \"normal\""
                    print indent "}"
                    inserted = 1
                }
            }
        ' "$base" >"$candidate"
    elif grep -Eq '^[[:space:]]*keybinds([[:space:]]|\{)' "$base"; then
        printf 'Cannot install dashboard binding: keybinds has no session block in %s\n' "$config" >&2
        exit 1
    else
        cat "$base" >"$candidate"
        cat >>"$candidate" <<EOF

keybinds {
    session {
        $marker
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
    fi
fi
ZELLIJ_CONFIG_FILE="$candidate" zellij setup --check >/dev/null
mv "$candidate" "$config"
rm -f "$base"
trap - EXIT HUP INT TERM

printf 'Installed dashboard: %s\n' "$plugin"
printf 'Installed Pi extension: %s -> %s\n' "$extension" "$repo/extensions/zellij-status.ts"
printf 'Restart Pi and Zellij, then press Ctrl-o followed by uppercase P.\n'
