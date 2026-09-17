Craft official release v1.0.0: native high-performance Minecraft server supervisor, multi-platform runner, remote TUI, and backup engine.

### SHA-256 Checksums

| Asset | SHA-256 Checksum |
| :--- | :--- |
| `craft-linux-amd64` | `595536786106ccbf3e76deed698e8e0cc97174f0ab74e6fcbce451415dd3d1b6` |
| `craft-linux-amd64.tar.gz` | `10a201af7b42520f89fc16b33b67948bbc3f43aa834491657eafb0a584ae7fe2` |
| `craft-linux-amd64.gz` | `e93896d8d26c5bc1669854a7711db9b11fb19aeca75ba989022c1c4ebd14ead0` |
| `craft-windows-amd64.exe` | `66de6465e24a2713afc9e45fe5a82985a1de660fb90e3b8966c0dac48f1ae8b9` |
| `craft-windows-amd64.zip` | `aaf32e17a04c7e978066b200b1859717579a4837fda6a12b784c1fb06befe153` |
| `craft-darwin-arm64.tar.gz` | `79a50eae42633e5513d100c9f90e9d7f99645b4c29b5a364ac69609fc2a82efe` |
| `craft-darwin-amd64.tar.gz` | `742a11c57af41ff5c12469b730e1746c6889fb111b7d39ac0b64830afdba94ed` |

Verify any downloaded binary:
```bash
# Linux / macOS
sha256sum -c craft-linux-amd64.sha256

# Windows PowerShell
Get-FileHash .\craft-windows-amd64.exe -Algorithm SHA256
```
