#!/bin/sh
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
repo="$tmp/repo"
mkdir -p "$repo/target/wasm32-wasip1/release" "$repo/extensions" "$tmp/bin"
cp "$root/install.sh" "$repo/install.sh"
: >"$repo/target/wasm32-wasip1/release/zellij-pi-dashboard.wasm"
: >"$repo/extensions/zellij-status.ts"
printf '#!/bin/sh\nexit 0\n' >"$tmp/bin/cargo"
printf '#!/bin/sh\nexit 0\n' >"$tmp/bin/zellij"
chmod +x "$tmp/bin/cargo" "$tmp/bin/zellij"

install_case() {
    home="$tmp/$1"
    mkdir -p "$home/.config/zellij"
    cat >"$home/.config/zellij/config.kdl"
    HOME="$home" PATH="$tmp/bin:$PATH" "$repo/install.sh" >/dev/null
}

install_case empty </dev/null
[ "$(grep -c '^keybinds' "$tmp/empty/.config/zellij/config.kdl")" -eq 1 ]
grep -q 'zellij-pi-dashboard.wasm' "$tmp/empty/.config/zellij/config.kdl"

install_case existing <<'EOF'
keybinds clear-defaults=true {
    session {
        bind "p" { SwitchToMode "normal"; }
    }
}
EOF
existing="$tmp/existing/.config/zellij/config.kdl"
[ "$(grep -c '^keybinds' "$existing")" -eq 1 ]
[ "$(grep -c 'zellij-pi-dashboard.wasm' "$existing")" -eq 1 ]
HOME="$tmp/existing" PATH="$tmp/bin:$PATH" "$repo/install.sh" >/dev/null
[ "$(grep -c 'zellij-pi-dashboard.wasm' "$existing")" -eq 1 ]

install_case migrate <<'EOF'
keybinds clear-defaults=true {
    session {
        bind "p" { SwitchToMode "normal"; }
    }
}

// zellij-pi-dashboard
keybinds {
    session {
        bind "P" {
            LaunchOrFocusPlugin "file:/old/zellij-pi-dashboard.wasm" {}
        }
    }
}
EOF
migrate="$tmp/migrate/.config/zellij/config.kdl"
[ "$(grep -c '^keybinds' "$migrate")" -eq 1 ]
[ "$(grep -c 'zellij-pi-dashboard.wasm' "$migrate")" -eq 1 ]
! grep -q '^// zellij-pi-dashboard$' "$migrate"

unsupported="$tmp/unsupported"
mkdir -p "$unsupported/.config/zellij"
printf 'keybinds {\n    pane {}\n}\n' >"$unsupported/.config/zellij/config.kdl"
if HOME="$unsupported" PATH="$tmp/bin:$PATH" "$repo/install.sh" >/dev/null 2>&1; then
    echo 'expected unsupported keymap to fail' >&2
    exit 1
fi

printf 'install config checks passed\n'
