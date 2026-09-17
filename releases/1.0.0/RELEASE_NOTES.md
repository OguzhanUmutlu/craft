Craft official release v1.0.0: native high-performance Minecraft server supervisor, multi-platform runner, remote TUI, and backup engine.

### SHA-256 Checksums

| Asset | SHA-256 Checksum |
| :--- | :--- |
| `craft-linux-amd64` | `34c65f93bfb0ae280a647a17abf3b43dfa5e0c25b1a8436e78fd409111c41086` |
| `craft-linux-amd64.tar.gz` | `3d919bf9f2780ba1369f844bd67e39141deb34834bb87580dcbe94060c52c39e` |
| `craft-linux-amd64.gz` | `59b2ea902cee8c30044c4e9a2c7b13b625fb0d002cf203aa6299fe71bbb383d2` |
| `craft-windows-amd64.exe` | `66de6465e24a2713afc9e45fe5a82985a1de660fb90e3b8966c0dac48f1ae8b9` |
| `craft-windows-amd64.zip` | `9293ccbe72f104bb7bb969bd043f51a0fbd6f8ac63d79e8d129c6a094cf07123` |
| `craft-darwin-arm64.tar.gz` | `6656a64328c0e9bc1e099364e0f67ce5834b89cebc21e386b4d311c9ad595b70` |
| `craft-darwin-amd64.tar.gz` | `996cb522b4961348257aab44897e77bc5f98efc0bc7d75a007957b842fecf986` |

Verify any downloaded binary:
```bash
# Linux / macOS
sha256sum -c craft-linux-amd64.sha256

# Windows PowerShell
Get-FileHash .\craft-windows-amd64.exe -Algorithm SHA256
```
