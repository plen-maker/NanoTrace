#!/bin/sh
# Builds and installs the NanoTrace memory app on macOS or Arch (GNOME):
#   - ~/.local/bin/nanotrace
#   - starts `nanotrace --daemon` at login (sync at login + every 30 min)
#   - Ctrl+Alt+0 opens the status window
#       macOS: grabbed by the daemon itself
#       GNOME: a custom keyboard shortcut (Wayland apps can't grab global keys)
# Needs: cargo (rustup), and `gh auth login` done (or GITHUB_TOKEN set).
# Re-running it is safe (updates the binary, doesn't duplicate anything).
set -e
cd "$(dirname "$0")"
BIN="$HOME/.local/bin/nanotrace"
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "cargo kell: https://rustup.rs"; exit 1; }

cargo install --locked --path . --root "$HOME/.local"

case "$(uname -s)" in
Darwin)
  PLIST="$HOME/Library/LaunchAgents/com.plen-maker.nanotrace.plist"
  mkdir -p "$(dirname "$PLIST")" "$HOME/Library/Logs"
  cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.plen-maker.nanotrace</string>
  <key>ProgramArguments</key><array><string>$BIN</string><string>--daemon</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>$HOME/Library/Logs/nanotrace.log</string>
</dict>
</plist>
EOF
  launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
  launchctl bootstrap "gui/$(id -u)" "$PLIST"
  echo "Kész: a háttérfolyamat fut és bejelentkezéskor indul, Ctrl+Alt+0 nyitja az ablakot."
  ;;
Linux)
  mkdir -p "$HOME/.config/autostart"
  cat > "$HOME/.config/autostart/nanotrace.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=NanoTrace memory sync
Exec=$BIN --daemon
X-GNOME-Autostart-enabled=true
NoDisplay=true
EOF
  if command -v gsettings >/dev/null; then
    KB=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/nanotrace/
    LIST=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
    case "$LIST" in
      *"$KB"*) ;;
      *) NEW=$(python3 -c 'import ast,sys; l=ast.literal_eval(sys.argv[1].replace("@as ","")); l.append(sys.argv[2]); print(l)' "$LIST" "$KB")
         gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW" ;;
    esac
    S="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KB"
    gsettings set "$S" name 'NanoTrace'
    gsettings set "$S" command "$BIN"
    gsettings set "$S" binding '<Control><Alt>0'
  else
    echo "Nincs gsettings (nem GNOME?) - a Ctrl+Alt+0-t állítsd be kézzel erre: $BIN"
  fi
  pkill -f "$BIN --daemon" 2>/dev/null || true
  nohup "$BIN" --daemon >/dev/null 2>&1 &
  echo "Kész: a háttérfolyamat fut és bejelentkezéskor indul, Ctrl+Alt+0 nyitja az ablakot."
  ;;
*) echo "Csak macOS és Linux támogatott."; exit 1 ;;
esac
