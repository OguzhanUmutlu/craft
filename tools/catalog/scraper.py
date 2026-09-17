#!/usr/bin/env python3
"""
Craft Server Version Catalog Scraper & Aggregator
Scrapes upstream APIs and merges with historical archives for all 21 supported softwares.
Outputs:
  - Updated data/*.json files for embedded runtime fallbacks
  - tools/catalog/output/catalog.json (complete unified catalog)
  - tools/catalog/output/<software>.json (individual software manifests)
  - tools/catalog/output/versions.zst (zstd-compressed catalog for distribution)
"""

import os
import sys
import json
import time
import urllib.request
import urllib.error
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

USER_AGENT = "Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft; catalog-scraper)"

def fetch_json(url, timeout=12, headers=None):
    req_headers = {"User-Agent": USER_AGENT}
    if headers:
        req_headers.update(headers)
    req = urllib.request.Request(url, headers=req_headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        print(f"[WARN] Failed to fetch {url}: {e}", file=sys.stderr)
        return None

def fetch_xml(url, timeout=12):
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.read().decode("utf-8")
    except Exception as e:
        print(f"[WARN] Failed to fetch {url}: {e}", file=sys.stderr)
        return None

def load_json_file(path):
    p = Path(path)
    if p.exists():
        try:
            with open(p, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception as e:
            print(f"[WARN] Failed to load {path}: {e}", file=sys.stderr)
    return {}

def save_json_file(path, data):
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    with open(p, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)

# ==============================================================================
# 1. Vanilla Java Scraper
# ==============================================================================
def scrape_vanilla_java(historical_path, existing_data_path):
    print("Scraping Vanilla Java...")
    manifest = fetch_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
    results = {}
    assets_map = {}
    
    # 1. Load historical definitions (1.0 to 1.2.4, beta, alpha)
    historical = load_json_file(historical_path)
    for ver, url in historical.items():
        results[ver] = url
        assets_map[ver] = [{
            "filename": "server.jar",
            "url": url,
            "sha256": None,
            "is_archive": False
        }]

    if manifest and "versions" in manifest:
        # Filter release versions + recent snapshots
        release_entries = [v for v in manifest["versions"] if v.get("type") == "release"]
        snapshot_entries = [v for v in manifest["versions"] if v.get("type") == "snapshot"][:25]
        target_entries = release_entries + snapshot_entries

        def process_entry(entry):
            v_id = entry["id"]
            pkg_url = entry["url"]
            pkg_data = fetch_json(pkg_url, timeout=8)
            if pkg_data:
                server_info = pkg_data.get("downloads", {}).get("server")
                if server_info and "url" in server_info:
                    return v_id, server_info["url"], server_info.get("sha1")
            return v_id, None, None

        with ThreadPoolExecutor(max_workers=20) as executor:
            futures = [executor.submit(process_entry, entry) for entry in target_entries]
            for future in as_completed(futures):
                v_id, url, sha1 = future.result()
                if url:
                        results[v_id] = url
                        assets_map[v_id] = [{
                            "filename": "server.jar",
                            "url": url,
                            "sha256": None, # Mojang provides sha1
                            "is_archive": False
                        }]

    # Merge with existing file to avoid missing any older entries
    existing = load_json_file(existing_data_path)
    for k, v in existing.items():
        if k != "latest" and k not in results and isinstance(v, str) and v.startswith("http"):
            results[k] = v
            if k not in assets_map:
                assets_map[k] = [{
                    "filename": "server.jar",
                    "url": v,
                    "sha256": None,
                    "is_archive": False
                }]

    # Determine latest stable release
    latest_release = "1.21.4"
    if manifest and "latest" in manifest and "release" in manifest["latest"]:
        latest_release = manifest["latest"]["release"]

    data_output = {"latest": latest_release}
    # Sort keys descending
    sorted_vers = sorted([k for k in results.keys() if k != "latest"], reverse=True)
    for v in sorted_vers:
        data_output[v] = results[v]

    save_json_file(existing_data_path, data_output)
    print(f"  Vanilla Java complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest_release, assets_map

# ==============================================================================
# 2. PaperMC Scraper (Paper, Folia, Velocity, Waterfall)
# ==============================================================================
def scrape_papermc_project(project_id, historical_path=None, existing_data_path=None):
    print(f"Scraping PaperMC project: {project_id}...")
    url = f"https://fill.papermc.io/v3/projects/{project_id}"
    proj_data = fetch_json(url)
    results = {}
    assets_map = {}

    if proj_data and "versions" in proj_data:
        all_versions = []
        for group, vers in proj_data["versions"].items():
            all_versions.extend(vers)

        # Query latest build for each version (up to 40 recent versions)
        def process_version(v_str):
            b_url = f"https://fill.papermc.io/v3/projects/{project_id}/versions/{v_str}/builds"
            builds = fetch_json(b_url, timeout=8)
            if builds and isinstance(builds, list) and len(builds) > 0:
                chosen = None
                for b in builds:
                    if b.get("channel") == "STABLE":
                        chosen = b
                        break
                if not chosen:
                    chosen = builds[0]
                
                downloads = chosen.get("downloads", {})
                dl = downloads.get("server:default")
                if not dl:
                    dl = next(iter(downloads.values()), None)
                if dl and "url" in dl:
                    sha256 = dl.get("checksums", {}).get("sha256")
                    return v_str, dl["url"], sha256
            return v_str, None, None

        with ThreadPoolExecutor(max_workers=15) as executor:
            futures = [executor.submit(process_version, v) for v in all_versions]
            for future in as_completed(futures):
                v_str, dl_url, sha256 = future.result()
                if dl_url:
                        results[v_str] = dl_url
                        assets_map[v_str] = [{
                            "filename": f"{project_id}.jar",
                            "url": dl_url,
                            "sha256": sha256,
                            "is_archive": False
                        }]

    # Merge historical if available
    if historical_path:
        hist = load_json_file(historical_path)
        for k, v in hist.items():
            if k not in results and isinstance(v, str):
                results[k] = v
                assets_map[k] = [{
                    "filename": f"{project_id}.jar",
                    "url": v,
                    "sha256": None,
                    "is_archive": False
                }]

    # Merge existing data file
    if existing_data_path:
        existing = load_json_file(existing_data_path)
        for k, v in existing.items():
            if k != "latest" and k not in results and isinstance(v, str) and v.startswith("http"):
                results[k] = v
                if k not in assets_map:
                    assets_map[k] = [{
                        "filename": f"{project_id}.jar",
                        "url": v,
                        "sha256": None,
                        "is_archive": False
                    }]

    sorted_vers = sorted([k for k in results.keys() if k != "latest"], reverse=True)
    latest = sorted_vers[0] if sorted_vers else "latest"

    if existing_data_path:
        out = {"latest": latest}
        for v in sorted_vers:
            out[v] = results[v]
        save_json_file(existing_data_path, out)

    print(f"  {project_id} complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 3. Purpur Scraper
# ==============================================================================
def scrape_purpur(existing_data_path):
    print("Scraping Purpur...")
    proj = fetch_json("https://api.purpurmc.org/v2/purpur")
    results = {}
    assets_map = {}

    if proj and "versions" in proj:
        versions = proj["versions"]
        for v in versions:
            url = f"https://api.purpurmc.org/v2/purpur/{v}/latest/download"
            results[v] = url
            assets_map[v] = [{
                "filename": "purpur.jar",
                "url": url,
                "sha256": None,
                "is_archive": False
            }]

    existing = load_json_file(existing_data_path)
    for k, v in existing.items():
        if k != "latest" and k not in results and isinstance(v, str) and v.startswith("http"):
            results[k] = v
            assets_map[k] = [{
                "filename": "purpur.jar",
                "url": v,
                "sha256": None,
                "is_archive": False
            }]

    sorted_vers = sorted([k for k in results.keys() if k != "latest"], reverse=True)
    latest = sorted_vers[0] if sorted_vers else "1.21.4"

    out = {"latest": latest}
    for v in sorted_vers:
        out[v] = results[v]
    save_json_file(existing_data_path, out)

    print(f"  Purpur complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 4. Fabric Scraper
# ==============================================================================
def scrape_fabric():
    print("Scraping Fabric...")
    game_versions = fetch_json("https://meta.fabricmc.net/v2/versions/game")
    loaders = fetch_json("https://meta.fabricmc.net/v2/versions/loader")
    installers = fetch_json("https://meta.fabricmc.net/v2/versions/installer")

    latest_loader = loaders[0]["version"] if loaders and len(loaders) > 0 else "0.16.10"
    latest_installer = installers[0]["version"] if installers and len(installers) > 0 else "1.0.1"

    results = {}
    assets_map = {}
    sorted_vers = []

    if game_versions and isinstance(game_versions, list):
        for g in game_versions:
            v_id = g["version"]
            url = f"https://meta.fabricmc.net/v2/versions/loader/{v_id}/{latest_loader}/{latest_installer}/server/jar"
            results[v_id] = url
            assets_map[v_id] = [{
                "filename": "fabric-server.jar",
                "url": url,
                "sha256": None,
                "is_archive": False
            }]
            sorted_vers.append(v_id)

    latest = next((g["version"] for g in (game_versions or []) if g.get("stable")), "1.21.4")
    print(f"  Fabric complete: {len(results)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 5. Quilt Scraper
# ==============================================================================
def scrape_quilt():
    print("Scraping Quilt...")
    game_versions = fetch_json("https://meta.quiltmc.org/v3/versions/game")
    results = {}
    assets_map = {}
    sorted_vers = []

    if game_versions and isinstance(game_versions, list):
        for g in game_versions:
            v_id = g["version"]
            url = f"https://meta.quiltmc.org/v3/versions/loader/{v_id}/0.26.1/server/json"
            results[v_id] = url
            assets_map[v_id] = [{
                "filename": "quilt-server-launch.jar",
                "url": f"https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/0.11.0/quilt-installer-0.11.0.jar",
                "sha256": None,
                "is_archive": False
            }]
            sorted_vers.append(v_id)

    latest = next((g["version"] for g in (game_versions or []) if g.get("stable")), "1.21.4")
    print(f"  Quilt complete: {len(results)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 6. NeoForge Scraper
# ==============================================================================
def scrape_neoforge():
    print("Scraping NeoForge...")
    xml_data = fetch_xml("https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml")
    versions = []
    assets_map = {}

    if xml_data:
        import xml.etree.ElementTree as ET
        try:
            root = ET.fromstring(xml_data)
            for v_elem in root.findall(".//version"):
                v = v_elem.text
                if v:
                    versions.append(v)
        except Exception as e:
            print(f"[WARN] Failed to parse NeoForge maven XML: {e}", file=sys.stderr)

    if not versions:
        versions = ["21.4.38-beta", "21.1.92", "21.0.167", "20.6.119", "20.4.237", "20.2.86"]

    sorted_vers = sorted(versions, reverse=True)
    latest = sorted_vers[0]

    for v in sorted_vers:
        url = f"https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-installer.jar"
        assets_map[v] = [{
            "filename": "neoforge-installer.jar",
            "url": url,
            "sha256": None,
            "is_archive": False
        }]

    print(f"  NeoForge complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 7. Spigot Scraper & Historical CraftBukkit
# ==============================================================================
def scrape_spigot(craftbukkit_hist_path, existing_data_path):
    print("Scraping Spigot & historical CraftBukkit...")
    results = {}
    assets_map = {}

    # Historical CraftBukkit (1.2.5, 1.4.7, 1.5.2, 1.6.4, 1.7.2, 1.7.10)
    cb_hist = load_json_file(craftbukkit_hist_path)
    for k, v in cb_hist.items():
        results[k] = v
        assets_map[k] = [{
            "filename": "spigot.jar",
            "url": v,
            "sha256": None,
            "is_archive": False
        }]

    # Existing Spigot mappings
    existing = load_json_file(existing_data_path)
    for k, v in existing.items():
        if k != "latest" and isinstance(v, str):
            results[k] = v
            assets_map[k] = [{
                "filename": "spigot.jar",
                "url": v,
                "sha256": None,
                "is_archive": False
            }]

    sorted_vers = sorted([k for k in results.keys() if k != "latest"], reverse=True)
    latest = sorted_vers[0] if sorted_vers else "1.21.4"

    out = {"latest": latest}
    for v in sorted_vers:
        out[v] = results[v]
    save_json_file(existing_data_path, out)

    print(f"  Spigot complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 8. PocketMine-MP Scraper
# ==============================================================================
def scrape_pocketmine(existing_data_path):
    print("Scraping PocketMine-MP...")
    existing = load_json_file(existing_data_path)
    results = {}
    assets_map = {}

    # Load existing structured entries first
    for k, v in existing.items():
        if k == "latest":
            continue
        if isinstance(v, list) and len(v) >= 4:
            results[k] = v[1]
            assets_map[k] = [
                {"filename": "PocketMine-MP.phar", "url": v[1], "sha256": None, "is_archive": False},
                {"filename": "start.sh", "url": v[2], "sha256": None, "is_archive": False},
                {"filename": "start.cmd", "url": v[3], "sha256": None, "is_archive": False},
            ]
        elif isinstance(v, str):
            results[k] = v
            assets_map[k] = [{
                "filename": "PocketMine-MP.phar",
                "url": v,
                "sha256": None,
                "is_archive": False
            }]

    # Optionally fetch newer releases from GitHub
    releases = fetch_json("https://api.github.com/repos/pmmp/PocketMine-MP/releases?per_page=100")
    if releases and isinstance(releases, list):
        for r in releases:
            tag = r.get("tag_name", "").lstrip("v")
            if not tag or tag in results:
                continue
            assets = r.get("assets", [])
            phar = next((a for a in assets if a.get("name", "").endswith(".phar")), None)
            sh = next((a for a in assets if a.get("name", "") == "start.sh"), None)
            cmd = next((a for a in assets if a.get("name", "") == "start.cmd"), None)
            if phar and "browser_download_url" in phar:
                results[tag] = phar["browser_download_url"]
                assets_map[tag] = [
                    {"filename": "PocketMine-MP.phar", "url": phar["browser_download_url"], "sha256": None, "is_archive": False}
                ]
                if sh and "browser_download_url" in sh:
                    assets_map[tag].append({"filename": "start.sh", "url": sh["browser_download_url"], "sha256": None, "is_archive": False})
                if cmd and "browser_download_url" in cmd:
                    assets_map[tag].append({"filename": "start.cmd", "url": cmd["browser_download_url"], "sha256": None, "is_archive": False})

    sorted_vers = sorted([k for k in results.keys() if k != "latest"], reverse=True)
    latest = existing.get("latest", sorted_vers[0] if sorted_vers else "1.21.111")

    print(f"  PocketMine-MP complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map


# ==============================================================================
# 9. WaterdogPE Scraper
# ==============================================================================
def scrape_waterdog():
    print("Scraping WaterdogPE...")
    releases = fetch_json("https://api.github.com/repos/WaterdogPE/WaterdogPE/releases?per_page=50")
    results = {}
    assets_map = {}

    if releases and isinstance(releases, list):
        for r in releases:
            tag = r.get("tag_name", "").lstrip("v")
            if not tag:
                continue
            assets = r.get("assets", [])
            jar = next((a for a in assets if a.get("name", "").endswith(".jar")), None)
            if jar and "browser_download_url" in jar:
                results[tag] = jar["browser_download_url"]
                assets_map[tag] = [{
                    "filename": "waterdog.jar",
                    "url": jar["browser_download_url"],
                    "sha256": None,
                    "is_archive": False
                }]

    if not results:
        results["2.0.2"] = "https://github.com/WaterdogPE/WaterdogPE/releases/download/v2.0.2/Waterdog.jar"
        assets_map["2.0.2"] = [{
            "filename": "waterdog.jar",
            "url": results["2.0.2"],
            "sha256": None,
            "is_archive": False
        }]

    sorted_vers = sorted(results.keys(), reverse=True)
    latest = sorted_vers[0]
    print(f"  WaterdogPE complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 10. Terraria / TShock Scraper
# ==============================================================================
def scrape_terraria():
    print("Scraping Terraria / TShock...")
    releases = fetch_json("https://api.github.com/repos/Pryaxis/TShock/releases?per_page=100")
    results = {}
    assets_map = {}

    if releases and isinstance(releases, list):
        for r in releases:
            tag = r.get("tag_name", "").lstrip("v")
            if not tag:
                continue
            assets = r.get("assets", [])
            zip_asset = next((a for a in assets if a.get("name", "").endswith(".zip")), None)
            if zip_asset and "browser_download_url" in zip_asset:
                results[tag] = zip_asset["browser_download_url"]
                assets_map[tag] = [{
                    "filename": "tshock-release.zip",
                    "url": zip_asset["browser_download_url"],
                    "sha256": None,
                    "is_archive": True
                }]

    if not results:
        results["5.2.0"] = "https://github.com/Pryaxis/TShock/releases/download/v5.2.0/TShock-5.2.0-for-Terraria-1.4.4.9-linux-x64.zip"
        assets_map["5.2.0"] = [{
            "filename": "tshock.zip",
            "url": results["5.2.0"],
            "sha256": None,
            "is_archive": True
        }]

    sorted_vers = sorted(results.keys(), reverse=True)
    latest = sorted_vers[0]
    print(f"  Terraria complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# 11. Vanilla Bedrock BDS Scraper
# ==============================================================================
def scrape_bedrock(linux_data_path, windows_data_path):
    print("Scraping Vanilla Bedrock BDS...")
    linux_data = load_json_file(linux_data_path)
    windows_data = load_json_file(windows_data_path)

    linux_vers = [k for k in linux_data.keys() if k != "latest"]
    windows_vers = [k for k in windows_data.keys() if k != "latest"]

    sorted_vers = sorted(list(set(linux_vers + windows_vers)), reverse=True)
    latest = linux_data.get("latest", sorted_vers[0] if sorted_vers else "1.21.60.10")

    assets_map = {}
    for v in sorted_vers:
        l_url = linux_data.get(v, f"https://www.minecraft.net/bedrockdedicatedserver/bin-linux/bedrock-server-{v}.zip")
        assets_map[v] = [{
            "filename": "bedrock-server.zip",
            "url": l_url,
            "sha256": None,
            "is_archive": True
        }]

    print(f"  Vanilla Bedrock complete: {len(sorted_vers)} versions indexed.")
    return sorted_vers, latest, assets_map

# ==============================================================================
# Main Builder & Catalog Publisher
# ==============================================================================
def main():
    repo_root = Path(__file__).resolve().parent.parent.parent
    data_dir = repo_root / "data"
    hist_dir = repo_root / "tools" / "catalog" / "historical"
    output_dir = repo_root / "tools" / "catalog" / "output"
    output_dir.mkdir(parents=True, exist_ok=True)

    print("================================================================================")
    print("Craft Master Server Software Catalog Builder")
    print("================================================================================")

    # 1. Scrape each software platform
    v_java_vers, v_java_latest, v_java_assets = scrape_vanilla_java(
        hist_dir / "vanilla_java.json",
        data_dir / "vanilla-java-versions.json"
    )

    paper_vers, paper_latest, paper_assets = scrape_papermc_project(
        "paper",
        hist_dir / "paperspigot.json",
        data_dir / "paper-versions.json"
    )

    folia_vers, folia_latest, folia_assets = scrape_papermc_project(
        "folia",
        None,
        data_dir / "folia-versions.json"
    )

    velocity_vers, velocity_latest, velocity_assets = scrape_papermc_project("velocity")
    waterfall_vers, waterfall_latest, waterfall_assets = scrape_papermc_project("waterfall")

    purpur_vers, purpur_latest, purpur_assets = scrape_purpur(data_dir / "purpur-versions.json")
    fabric_vers, fabric_latest, fabric_assets = scrape_fabric()
    quilt_vers, quilt_latest, quilt_assets = scrape_quilt()
    neoforge_vers, neoforge_latest, neoforge_assets = scrape_neoforge()
    spigot_vers, spigot_latest, spigot_assets = scrape_spigot(hist_dir / "craftbukkit.json", data_dir / "spigot-versions.json")
    pocketmine_vers, pocketmine_latest, pocketmine_assets = scrape_pocketmine(data_dir / "pocketmine-versions.json")
    waterdog_vers, waterdog_latest, waterdog_assets = scrape_waterdog()
    terraria_vers, terraria_latest, terraria_assets = scrape_terraria()
    bedrock_vers, bedrock_latest, bedrock_assets = scrape_bedrock(
        data_dir / "vanilla-bedrock-linux-versions.json",
        data_dir / "vanilla-bedrock-windows-versions.json"
    )

    # 2. Assemble Master Catalog Structure
    catalog = {
        "schema_version": 1,
        "generated_at": int(time.time()),
        "softwares": {}
    }

    software_defs = [
        ("vanilla_java", "Vanilla (Java)", "minecraft", "Java", "Official Mojang Java dedicated server", "server.jar", v_java_vers, v_java_latest, v_java_assets),
        ("paper", "Paper", "minecraft", "Java", "High-performance standard Java server", "paper.jar", paper_vers, paper_latest, paper_assets),
        ("purpur", "Purpur", "minecraft", "Java", "Paper fork with extensive gameplay tweaks", "purpur.jar", purpur_vers, purpur_latest, purpur_assets),
        ("folia", "Folia", "minecraft", "Java", "Multi-threaded regional ticking server", "folia.jar", folia_vers, folia_latest, folia_assets),
        ("fabric", "Fabric", "minecraft", "Java", "Lightweight modular modded server", "fabric-server.jar", fabric_vers, fabric_latest, fabric_assets),
        ("quilt", "Quilt", "minecraft", "Java", "Community-driven modular modded server", "quilt-server-launch.jar", quilt_vers, quilt_latest, quilt_assets),
        ("neoforge", "NeoForge", "minecraft", "Java", "Modern Forge-compatible modded server", "neoforge-server.jar", neoforge_vers, neoforge_latest, neoforge_assets),
        ("spigot", "Spigot", "minecraft", "Java", "Classic Bukkit / Spigot plugin server", "spigot.jar", spigot_vers, spigot_latest, spigot_assets),
        ("velocity", "Velocity", "minecraft", "Proxy", "Next-generation ultra-fast proxy", "velocity.jar", velocity_vers, velocity_latest, velocity_assets),
        ("waterfall", "Waterfall", "minecraft", "Proxy", "Optimized BungeeCord proxy fork", "waterfall.jar", waterfall_vers, waterfall_latest, waterfall_assets),
        ("bungeecord", "BungeeCord", "minecraft", "Proxy", "Classic multi-server network proxy", "BungeeCord.jar", ["latest", "1800", "1700"], "latest", {
            "latest": [{"filename": "BungeeCord.jar", "url": "https://ci.md-5.net/job/BungeeCord/lastSuccessfulBuild/artifact/bootstrap/target/BungeeCord.jar", "sha256": None, "is_archive": False}]
        }),
        ("geyser", "GeyserMC Standalone", "minecraft", "Proxy", "Cross-play bridge for Bedrock clients", "Geyser.jar", ["latest"], "latest", {
            "latest": [{"filename": "Geyser.jar", "url": "https://download.geysermc.org/v2/projects/geyser/versions/latest/builds/latest/downloads/standalone", "sha256": None, "is_archive": False}]
        }),
        ("vanilla_bedrock", "Vanilla Bedrock BDS", "minecraft", "Bedrock", "Official Mojang Bedrock Dedicated Server", "bedrock_server", bedrock_vers, bedrock_latest, bedrock_assets),
        ("pocketmine", "PocketMine-MP", "minecraft", "Bedrock", "High-performance C++ / PHP Bedrock server", "PocketMine-MP.phar", pocketmine_vers, pocketmine_latest, pocketmine_assets),
        ("nukkit", "NukkitX", "minecraft", "Bedrock", "Java-based multi-threaded Bedrock server", "nukkit.jar", ["2.0.0", "1.0.0"], "2.0.0", {
            "2.0.0": [{"filename": "nukkit.jar", "url": "https://ci.opencollab.dev/job/NukkitX/job/Nukkit/job/master/lastSuccessfulBuild/artifact/target/nukkit-1.0-SNAPSHOT.jar", "sha256": None, "is_archive": False}]
        }),
        ("waterdog", "WaterdogPE", "minecraft", "Proxy", "Native Bedrock network proxy", "waterdog.jar", waterdog_vers, waterdog_latest, waterdog_assets),
        ("terraria", "Terraria", "terraria", "Native", "Terraria TShock Dedicated Server", "TerrariaServer.exe", terraria_vers, terraria_latest, terraria_assets),
        ("palworld", "Palworld", "palworld", "Native", "Palworld Dedicated Server (SteamCMD)", "PalServer.sh", ["latest"], "latest", {
            "latest": [{"filename": "PalServer.sh", "url": "steam://2394010", "sha256": None, "is_archive": False}]
        }),
        ("valheim", "Valheim", "valheim", "Native", "Valheim Dedicated Server (SteamCMD)", "valheim_server.x86_64", ["latest"], "latest", {
            "latest": [{"filename": "valheim_server.x86_64", "url": "steam://896660", "sha256": None, "is_archive": False}]
        }),
        ("factorio", "Factorio", "factorio", "Native", "Factorio Headless Dedicated Server", "factorio", ["2.0.77", "2.1.19", "1.1.110"], "2.0.77", {
            "2.0.77": [{"filename": "factorio_headless.tar.xz", "url": "https://factorio.com/get-download/2.0.77/headless/linux64", "sha256": None, "is_archive": True}],
            "2.1.19": [{"filename": "factorio_headless.tar.xz", "url": "https://factorio.com/get-download/2.1.19/headless/linux64", "sha256": None, "is_archive": True}]
        })
    ]

    total_sw = len(software_defs)
    total_ver = 0

    for sw_id, name, game_id, edition, desc, sfile, vers, latest, assets in software_defs:
        total_ver += len(vers)
        entry = {
            "id": sw_id,
            "name": name,
            "game_id": game_id,
            "edition": edition,
            "description": desc,
            "default_server_file": sfile,
            "latest_version": latest,
            "recommended_version": latest,
            "versions": vers,
            "assets": assets,
            "metadata": {}
        }
        catalog["softwares"][sw_id] = entry

        # Save individual software file
        save_json_file(output_dir / f"{sw_id}.json", entry)

    # Save master catalog.json
    save_json_file(output_dir / "catalog.json", catalog)

    print("================================================================================")
    print(f"Catalog generation complete: {total_sw} softwares, {total_ver} total versions.")
    print("Compressing master catalog with zstandard (level 19)...")

    # Compress catalog to versions.zst using zstd CLI
    raw_catalog_file = output_dir / "catalog.json"
    zst_catalog_file = output_dir / "versions.zst"
    try:
        subprocess.run(["zstd", "-19", "-f", str(raw_catalog_file), "-o", str(zst_catalog_file)], check=True)
        raw_size = raw_catalog_file.stat().st_size
        zst_size = zst_catalog_file.stat().st_size
        ratio = (1.0 - zst_size / raw_size) * 100.0
        print(f"  Raw catalog:  {raw_size:,} bytes ({raw_size/1024:.1f} KB)")
        print(f"  Zstd catalog: {zst_size:,} bytes ({zst_size/1024:.1f} KB) - Compression ratio: {ratio:.1f}%")
    except Exception as e:
        print(f"[ERROR] Failed to zstd-compress catalog: {e}", file=sys.stderr)
        sys.exit(1)

    print(f"All files saved to {output_dir}/")
    print("Ready for GitHub release publication.")

if __name__ == "__main__":
    main()
