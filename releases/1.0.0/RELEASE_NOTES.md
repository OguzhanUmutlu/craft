Craft official release v1.0.0: native high-performance Minecraft server supervisor, multi-platform runner, remote TUI, and backup engine.

### SHA-256 Checksums

| Asset | SHA-256 Checksum |
| :--- | :--- |
| `craft-linux-amd64` | `6793327da40b89db402970bfbff99697a336b903baa1e0c2683ab91a502b8754` |
| `craft-linux-amd64.tar.gz` | `45cc47e7d6314b343f6214f8ec026b1b6181d5655dc13b4d5bc235f2d5d37aab` |
| `craft-linux-amd64.gz` | `71aa6d7363905bfcae415f9dc65ec3750a67b219fba88f56f822931cfb7a9e75` |
| `craft-windows-amd64.exe` | `66de6465e24a2713afc9e45fe5a82985a1de660fb90e3b8966c0dac48f1ae8b9` |
| `craft-windows-amd64.zip` | `6f89f79934414035eb8c8a319e0e6981aefd348b6d39a319cada9401fe57974b` |
| `craft-darwin-arm64.tar.gz` | `e64cc578d2be81dcf0567e2ae25b52a4516245d9a96e9eeb18579d5c0dd217fc` |
| `craft-darwin-amd64.tar.gz` | `b1e9fccbc4d327d443ebcd7923947ae3124164efe6eca586108b174481a387c3` |

Verify any downloaded binary:
```bash
# Linux / macOS
sha256sum -c craft-linux-amd64.sha256

# Windows PowerShell
Get-FileHash .\craft-windows-amd64.exe -Algorithm SHA256
```
