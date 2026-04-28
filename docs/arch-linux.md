# Arch Linux Installation

Roon AI is available for Arch Linux and Arch-based distributions (RoPieee, AudioLinux, etc.).

## Installation from AUR

### Using an AUR helper (recommended)

```bash
# Using yay
yay -S roon-ai-bin

# Using paru
paru -S roon-ai-bin
```

### Manual installation

```bash
git clone https://aur.archlinux.org/roon-ai-bin.git
cd roon-ai-bin
makepkg -si
```

## Post-Installation

### Start the service

```bash
# Enable and start the service
sudo systemctl enable --now roon-ai

# Check status
sudo systemctl status roon-ai

# View logs
journalctl -u roon-ai -f
```

### Access the Web UI

Open your browser to: **http://localhost:8088**

## Configuration

Configuration files are stored in `/etc/roon-ai/`.

### Environment Variables

The systemd service supports these environment variables (edit the service file or use a drop-in):

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8088` | HTTP server port |
| `CONFIG_DIR` | `/etc/roon-ai` | Configuration directory |
| `DATA_DIR` | `/var/lib/roon-ai` | State/data directory |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |

To customize, create a drop-in:

```bash
sudo systemctl edit roon-ai
```

Add your overrides:

```ini
[Service]
Environment=PORT=9000
Environment=RUST_LOG=debug
```

## File Locations

| Path | Description |
|------|-------------|
| `/usr/bin/roon-ai` | Binary |
| `/usr/share/roon-ai/public/` | Web assets |
| `/etc/roon-ai/` | Configuration |
| `/var/lib/roon-ai/` | Runtime state, Roon tokens |
| `/usr/lib/systemd/system/roon-ai.service` | Systemd service |

## Uninstallation

```bash
# Using yay
yay -Rns roon-ai-bin

# Manual
sudo pacman -Rns roon-ai-bin
```

Configuration and state directories are preserved. Remove manually if no longer needed:

```bash
sudo rm -rf /etc/roon-ai
sudo rm -rf /var/lib/roon-ai
```

## Building from Source

If you prefer to build from source instead of using the binary package:

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Install Dioxus CLI
cargo install dioxus-cli

# Install Node.js (for Tailwind CSS)
sudo pacman -S nodejs npm
```

### Build

```bash
git clone https://github.com/cfogarty1964/Roon-AI.git
cd roon-ai
git checkout v3

# Build CSS
make css

# Build web assets
dx build --release --platform web --features web

# Build server binary
cargo build --release
```

The binary will be at `target/release/roon-ai`.

## RoPieee / AudioLinux Integration

For RoPieee and AudioLinux developers: this package follows standard Arch packaging conventions. The PKGBUILD can be adapted for inclusion in your distribution's package repository.

Key considerations:
- Binary is statically linked (musl) with no runtime dependencies
- Web assets are required at `/usr/share/roon-ai/public/` (symlinked to state dir)
- Systemd service uses `DynamicUser=yes` for security
- Configuration persists in `/etc/roon-ai/`
