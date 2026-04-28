## Installation

### Docker

```yaml
services:
  roon-ai:
    image: cfogarty1964/roon-ai:{{VERSION}}
    network_mode: host
    volumes:
      - ./data:/data
    environment:
      - CONFIG_DIR=/data
    restart: unless-stopped
```

```bash
docker compose up -d
# Access http://localhost:8088
```

### QNAP NAS

Download the QPKG package from the assets below:
- `roon-ai_*_x86_64.qpkg` — Intel/AMD x86_64
- `roon-ai_*_arm_64.qpkg` — ARM64

### Roon Extension Manager

Search for "Roon AI" in Roon Extension Manager and install.

### LMS Plugin

Add this repository URL in LMS Settings → Plugins → Additional Repositories:
```
https://raw.githubusercontent.com/cfogarty1964/Roon-AI/v3/lms-plugin/repo.xml
```
Then install "Roon AI" from the plugin list.

---

## MCP Server (Claude Integration)

The bridge includes a built-in MCP server. Add to your MCP config (Claude Code, Claude Desktop, etc.):

```json
{
  "mcpServers": {
    "roon-ai": {
      "type": "http",
      "url": "http://<your-bridge-host>:8088/mcp"
    }
  }
}
```

Replace `<your-bridge-host>` with your bridge IP or hostname (e.g., `localhost`, `192.168.1.100`, `nas.local`).

---

## Configuration

Configure all backends (Roon, LMS, HQPlayer, UPnP/OpenHome) via the web UI at `http://<your-bridge-host>:8088`.
