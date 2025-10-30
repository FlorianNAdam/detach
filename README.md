# detach

`detach` is a terminal utility that executes a command in a virtual terminal and displays its output dynamically at the bottom of the current terminal session. It provides a floating, live interface for commands without occupying the full terminal.

## Features

* Executes any command in a virtual terminal.
* Supports live ANSI rendering with colors, bold, underline, etc.
* Displays output at the bottom of the terminal without blocking existing content.
* Lightweight and fast.
* Can be installed via Cargo or used directly as a Nix flake.

## Installation

### Using Cargo

```bash
cargo install --path .
```

### Using Nix Flake

```bash
nix run github:FlorianNAdam/detach
```

## Usage

```bash
detach <command> [args...]
```

Example:

```bash
detach htop
detach cargo watch -x run
```

## License

MIT
