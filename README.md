# NanoTrace

Monorepo:

- **`benchpeek/`** - board-diagnostics cockpit for the STM32MP257F-DK (Rust/egui, WASM decoder plugins, DRM/KMS backend for the board's panel). Formerly `github.com/plen-maker/LogPeaker`; its full history is preserved here. Start with `benchpeek/CLAUDE.md`. Its old side branches live on as `benchpeek/mono-workspace-ui` and `benchpeek/automation-issue-driven-claude`.
- **`nanotrace/`** - memory-sync app: keeps Claude Code memory in sync with the private `plen-maker/nanotrace-vault` (background sync at login + every 30 min, **Ctrl+Alt+0** status window). macOS and Arch/GNOME.

## Install the memory-sync app (once per machine)

Needs rustup and `gh auth login`.

```sh
gh repo clone plen-maker/NanoTrace ~/NanoTrace
sh ~/NanoTrace/nanotrace/install.sh
```

Update later with `git -C ~/NanoTrace pull && sh ~/NanoTrace/nanotrace/install.sh`.
