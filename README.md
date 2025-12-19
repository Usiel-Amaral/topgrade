<div align="center">
  <h1>Topgrade GUI</h1>
  <h3>The Interactive Upgrade Tool for Linux</h3>
</div>

> **Note**
> **This is a FORK of [topgrade](https://github.com/r-darwish/topgrade) that adds a Graphical User Interface (GUI).**
> It allows you to run system updates with a visual feedback loop and interactive prompts (e.g., sudo passwords) directly in a window.

## GUI Features

- **Embedded Terminal**: Runs the upgrade process in a dedicated window.
- **Interactive Prompts**: Automatically detects password requests (`sudo`) and confirmation prompts (`[y/n]`, `[S/n]`), showing an input field for you to type without needing a separate terminal.
- **Type-to-Interact**: Just start typing to confirm actions or enter credentials.
- **Dark Mode**: Optimized for readability with high-contrast text.

## Introduction

Keeping your system up to date usually involves invoking multiple package managers.
**Topgrade** detects which tools you use and runs the appropriate commands to update them. This fork wraps that powerful logic in a user-friendly GUI.

## Installation / Download

### AppImage (Recommended for Linux)

Download the `.AppImage` file from the [Releases](https://github.com/Usiel-Amaral/topgrade/releases) page.
1. Make it executable: `chmod +x Topgrade_GUI-*.AppImage`
2. Run it: `./Topgrade_GUI-*.AppImage`

### Build from Source

```bash
cargo install --path . --bin topgrade-gui --features gui
```

## Usage

Run the application from your application menu or command line:
```bash
topgrade-gui
```

## Configuration

See [`config.example.toml`](https://github.com/topgrade-rs/topgrade/blob/main/config.example.toml) for configuration options. The GUI respects the same configuration files as the CLI tool.

---

*Original Topgrade Credits:*
> [topgrade by r-darwish](https://github.com/r-darwish/topgrade)
