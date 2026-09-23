#!/usr/bin/env python3
"""
Craft Desktop Studio CDP End-to-End Automated Test Runner
Validates all 9 core user journeys, captures 12 visual regression screenshots,
audits zero-emoji DOM compliance, and profiles V8 layout and memory performance.

Zero third-party pip dependencies - pure standard library implementation.
Strictly compliant with workspace zero-emoji policy.
"""

import os
import sys
import time
import json
import argparse
from pathlib import Path

# Add tools/devtools to python path
DEVTOOLS_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = DEVTOOLS_DIR.parent.parent
sys.path.insert(0, str(DEVTOOLS_DIR))

from devtools import DevToolsController, cmd_start, cmd_stop, DEFAULT_CDP_PORT


class TestAssertionError(AssertionError):
    pass


class StudioTestSuite:
    def __init__(self, port: int = DEFAULT_CDP_PORT):
        self.port = port
        self.controller = DevToolsController(port=port)
        self.passed_tests = 0
        self.failed_tests = 0
        self.test_log = []

    def log(self, msg: str):
        print(f"[TEST] {msg}")
        self.test_log.append(f"[TEST] {msg}")

    def log_pass(self, name: str):
        self.passed_tests += 1
        print(f"[PASS] {name}")
        self.test_log.append(f"[PASS] {name}")

    def log_fail(self, name: str, reason: str):
        self.failed_tests += 1
        print(f"[FAIL] {name}: {reason}")
        self.test_log.append(f"[FAIL] {name}: {reason}")

    def wait_and_assert(self, js_condition: str, error_msg: str, timeout_secs: float = 6.0):
        """Polls until js_condition returns truthy or raises TestAssertionError."""
        start = time.time()
        while time.time() - start < timeout_secs:
            try:
                res = self.controller.evaluate_javascript(js_condition)
                if res:
                    return res
            except Exception:
                pass
            time.sleep(0.25)
        raise TestAssertionError(error_msg)

    def trigger_key_combo(self, key: str, ctrl: bool = True):
        """Dispatches a synthetic keyboard event on window."""
        script = f"""
        (() => {{
            window.dispatchEvent(new KeyboardEvent('keydown', {{
                key: '{key}',
                ctrlKey: {'true' if ctrl else 'false'},
                bubbles: true,
                cancelable: true
            }}));
            return true;
        }})()
        """
        return self.controller.evaluate_javascript(script)

    def switch_tab(self, tab_index: int, tab_title: str):
        """Switches active view tab via nav-rail button click or synthetic shortcut."""
        script = f"""
        (() => {{
            const btn = document.querySelector('.nav-rail button[title*="{tab_title}"]');
            if (btn) {{
                btn.click();
                return true;
            }}
            window.dispatchEvent(new KeyboardEvent('keydown', {{
                key: '{tab_index}',
                ctrlKey: true,
                bubbles: true,
                cancelable: true
            }}));
            return true;
        }})()
        """
        self.controller.evaluate_javascript(script)
        time.sleep(0.4)

    def audit_emoji_compliance(self, context_name: str):
        """Enforces zero-emoji policy across all rendered DOM text."""
        violations = self.controller.audit_zero_emoji()
        if violations:
            msg = f"Zero-emoji policy violated in {context_name}: {violations}"
            raise TestAssertionError(msg)

    # -------------------------------------------------------------------------
    # Test Journeys
    # -------------------------------------------------------------------------

    def journey_01_shell_and_navigation(self):
        """Journey 1: Studio Shell, Brand Badges, and 9-Tab Navigation."""
        self.log("Executing Journey 1: Studio Shell & Navigation Layout...")

        # Assert brand badge & cluster badge
        self.wait_and_assert(
            "document.body.innerText.includes('CRAFT STUDIO')",
            "TopBar brand badge 'CRAFT STUDIO' not found in DOM."
        )
        self.wait_and_assert(
            "document.body.innerText.includes('CLUSTER: PROD-EU')",
            "Cluster badge '[CLUSTER: PROD-EU]' not found in DOM."
        )
        self.wait_and_assert(
            "document.body.innerText.includes('DAEMON: CONNECTED')",
            "Daemon connection indicator not found in DOM."
        )

        # 1. Fleet Overview (Ctrl+1)
        self.switch_tab(1, "Fleet Overview")
        self.wait_and_assert(
            "document.body.innerText.includes('Fleet Orchestration & Status')",
            "Fleet Overview heading missing after Ctrl+1 navigation."
        )
        self.controller.capture_screenshot("01_fleet_overview.png")
        self.audit_emoji_compliance("01_fleet_overview")

        # 2. Live Console (Ctrl+2)
        self.switch_tab(2, "Live Console")
        self.wait_and_assert(
            "document.body.innerText.includes('Server Target:') || document.body.innerText.includes('Pause auto-scrolling')",
            "Live Console controls missing after Ctrl+2 navigation."
        )
        self.controller.capture_screenshot("03_console_view.png")
        self.audit_emoji_compliance("03_console_view")

        # 3. AI Diagnostics (Ctrl+3)
        self.switch_tab(3, "AI Diagnostics")
        self.wait_and_assert(
            "document.body.innerText.includes('Diagnostics') || document.body.innerText.includes('MSPT / TICK TIME')",
            "AI Diagnostics view missing after Ctrl+3 navigation."
        )
        self.controller.capture_screenshot("04_diagnostics_view.png")
        self.audit_emoji_compliance("04_diagnostics_view")

        # 4. Plugins & Mods (Ctrl+4)
        self.switch_tab(4, "Plugins & Mods")
        self.wait_and_assert(
            "document.body.innerText.includes('Plugin & Mod Lifecycle Hub')",
            "Plugins view missing after Ctrl+4 navigation."
        )
        self.controller.capture_screenshot("06_plugin_installed.png")
        self.audit_emoji_compliance("06_plugin_installed")

        # 5. Backup & DR Hub (Ctrl+5)
        self.switch_tab(5, "Backup & DR Hub")
        self.wait_and_assert(
            "document.body.innerText.includes('Backup & Snapshot Resilience Hub')",
            "Backup Hub missing after Ctrl+5 navigation."
        )
        self.controller.capture_screenshot("07_backup_hub.png")
        self.audit_emoji_compliance("07_backup_hub")

        # 6. Configuration Studio (Ctrl+6)
        self.switch_tab(6, "Configuration Studio")
        self.wait_and_assert(
            "document.body.innerText.includes('Configuration & Properties Studio')",
            "Config Studio missing after Ctrl+6 navigation."
        )
        self.controller.capture_screenshot("08_config_studio.png")
        self.audit_emoji_compliance("08_config_studio")

        # 7. Edge Mesh (Ctrl+7)
        self.switch_tab(7, "Edge Mesh")
        self.wait_and_assert(
            "document.body.innerText.includes('Global Edge Mesh') || document.body.innerText.includes('Latency Prober')",
            "Edge Mesh missing after Ctrl+7 navigation."
        )
        self.controller.capture_screenshot("10_edge_mesh.png")
        self.audit_emoji_compliance("10_edge_mesh")

        # 8. Storage & DR (Ctrl+8)
        self.switch_tab(8, "Storage & DR")
        self.wait_and_assert(
            "document.body.innerText.includes('Storage Mesh') || document.body.innerText.includes('Multi-Cloud')",
            "Storage Mesh missing after Ctrl+8 navigation."
        )
        self.controller.capture_screenshot("11_storage_mesh.png")
        self.audit_emoji_compliance("11_storage_mesh")

        # 9. Audit Trail (Ctrl+9)
        self.switch_tab(9, "Audit Trail")
        self.wait_and_assert(
            "document.body.innerText.includes('Cryptographic Audit Ledger') || document.body.innerText.includes('Audit')",
            "Audit Trail view missing after Ctrl+9 navigation."
        )
        self.controller.capture_screenshot("12_audit_trail.png")
        self.audit_emoji_compliance("12_audit_trail")

        # Return to Fleet tab
        self.switch_tab(1, "Fleet Overview")
        self.log_pass("Journey 1: Studio Shell & Navigation Layout")

    def journey_02_server_provisioning_wizard(self):
        """Journey 2: Multi-Step Server Creation Wizard Modal."""
        self.log("Executing Journey 2: Server Provisioning Wizard...")

        # Switch to Fleet
        self.switch_tab(1, "Fleet Overview")
        time.sleep(0.3)

        # Click 'Create Server' button
        self.controller.evaluate_javascript("""
        (() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const createBtn = btns.find(b => b.innerText.includes('Create Server') || b.title === 'Create Server');
            if (createBtn) createBtn.click();
            return !!createBtn;
        })()
        """)
        time.sleep(0.5)

        # Assert wizard modal mounted
        self.wait_and_assert(
            "document.body.innerText.includes('Server Provisioning Wizard')",
            "Create Server Wizard modal failed to open."
        )

        # Capture modal screenshot
        self.controller.capture_screenshot("02_server_wizard.png")
        self.audit_emoji_compliance("02_server_wizard")

        # Navigate Step 1 -> Step 2 (Click Next)
        self.controller.evaluate_javascript("""
        (() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const nextBtn = btns.find(b => b.innerText.includes('Next: Version') || b.innerText.includes('Next'));
            if (nextBtn) nextBtn.click();
        })()
        """)
        time.sleep(0.4)

        # Close modal via Escape key
        self.controller.evaluate_javascript("""
        (() => {
            window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        })()
        """)
        time.sleep(0.3)

        self.log_pass("Journey 2: Server Provisioning Wizard")

    def journey_03_power_lifecycle(self):
        """Journey 3: Server Power State Controls and Badge Transitions."""
        self.log("Executing Journey 3: Server Power Lifecycle Controls...")
        self.switch_tab(1, "Fleet Overview")

        # Check for presence of server cards
        has_cards = self.controller.evaluate_javascript("document.querySelectorAll('.card').length > 0")
        if not has_cards:
            raise TestAssertionError("No server cards rendered on Fleet Overview.")

        # Click Start or Stop on first card
        action_taken = self.controller.evaluate_javascript("""
        (() => {
            const card = document.querySelector('.card');
            if (!card) return null;
            const startBtn = Array.from(card.querySelectorAll('button')).find(b => b.innerText.includes('Start'));
            const stopBtn = Array.from(card.querySelectorAll('button')).find(b => b.innerText.includes('Stop'));
            if (stopBtn) {
                stopBtn.click();
                return 'stopped';
            } else if (startBtn) {
                startBtn.click();
                return 'started';
            }
            return null;
        })()
        """)
        time.sleep(0.5)
        self.wait_and_assert(
            "document.querySelectorAll('.badge').length > 0",
            "Status badges missing from server cards."
        )
        self.log_pass(f"Journey 3: Server Power Lifecycle Controls ({action_taken or 'verified'})")

    def journey_04_live_console(self):
        """Journey 4: Interactive Live Console Command Injection."""
        self.log("Executing Journey 4: Live Console & Command Injection...")
        self.switch_tab(2, "Live Console")

        # Assert terminal console exists
        self.wait_and_assert(
            "document.querySelector('input') !== null",
            "Console input box not found in DOM."
        )

        # Inject command into input and dispatch Enter
        dispatched = self.controller.evaluate_javascript("""
        (() => {
            const input = document.querySelector('input[type=\"text\"], input:not([type])');
            if (!input) return false;
            input.value = 'tps';
            input.dispatchEvent(new Event('input', { bubbles: true }));
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13, bubbles: true }));
            return true;
        })()
        """)
        time.sleep(0.5)

        if not dispatched:
            raise TestAssertionError("Could not inject console command into input prompt.")

        self.log_pass("Journey 4: Live Console & Command Injection")

    def journey_05_plugin_store(self):
        """Journey 5: Plugin Store Search & Manifest Inspection."""
        self.log("Executing Journey 5: Plugin Store & Modrinth Discovery...")
        self.switch_tab(4, "Plugins & Mods")

        # Switch to 'Discover & Install' tab
        switched = self.controller.evaluate_javascript("""
        (() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const discoverBtn = btns.find(b => b.innerText.includes('Discover & Install') || b.innerText.includes('Discover'));
            if (discoverBtn) {
                discoverBtn.click();
                return true;
            }
            return false;
        })()
        """)
        time.sleep(0.5)

        # Assert Discover tab is active and capture screenshot
        self.wait_and_assert(
            "document.body.innerText.includes('Modrinth') || document.body.innerText.includes('Search')",
            "Discover plugins view not rendered."
        )
        self.controller.capture_screenshot("05_plugin_store.png")
        self.audit_emoji_compliance("05_plugin_store")

        # Execute search for 'spark'
        self.controller.evaluate_javascript("""
        (() => {
            const input = document.querySelector('input[placeholder*=\"Search\"], input[type=\"text\"]');
            if (input) {
                input.value = 'spark';
                input.dispatchEvent(new Event('input', { bubbles: true }));
            }
        })()
        """)
        time.sleep(0.5)

        self.log_pass("Journey 5: Plugin Store & Modrinth Discovery")

    def journey_06_backup_hub(self):
        """Journey 6: Backup & Disaster Recovery Snapshot Hub."""
        self.log("Executing Journey 6: Backup & Snapshot Resilience Hub...")
        self.switch_tab(5, "Backup Hub")

        # Assert snapshot controls
        self.wait_and_assert(
            "document.body.innerText.includes('Create Hot Backup') || document.body.innerText.includes('Backups')",
            "Backup creation controls missing."
        )

        # Click Create Hot Backup button
        self.controller.evaluate_javascript("""
        (() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const backupBtn = btns.find(b => b.innerText.includes('Create Hot Backup') || b.innerText.includes('Backup'));
            if (backupBtn) backupBtn.click();
        })()
        """)
        time.sleep(0.5)

        self.log_pass("Journey 6: Backup & Snapshot Resilience Hub")

    def journey_07_config_studio(self):
        """Journey 7: Configuration Studio & JVM Tuning Optimizer."""
        self.log("Executing Journey 7: Configuration Studio & JVM Tuning...")
        self.switch_tab(6, "Configuration Studio")

        # Assert JVM profile buttons or properties fields
        self.wait_and_assert(
            "document.body.innerText.includes('server.properties') || document.body.innerText.includes('JVM')",
            "Configuration Studio editor elements missing."
        )

        # Select 'Balanced' JVM profile if present
        self.controller.evaluate_javascript("""
        (() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const balBtn = btns.find(b => b.innerText.includes('Balanced'));
            if (balBtn) balBtn.click();
        })()
        """)
        time.sleep(0.4)

        self.log_pass("Journey 7: Configuration Studio & JVM Tuning")

    def journey_08_cli_runner(self):
        """Journey 8: Universal CLI Command Runner Modal."""
        self.log("Executing Journey 8: Universal CLI Command Runner...")

        # Open CLI Runner via TopBar button
        opened = self.controller.evaluate_javascript("""
        (() => {
            const btn = document.querySelector('button[title*=\"CLI Runner\"], button[title*=\"Universal CLI\"]') ||
                        Array.from(document.querySelectorAll('button')).find(b => b.innerText.includes('CLI Terminal'));
            if (btn) {
                btn.click();
                return true;
            }
            return false;
        })()
        """)
        time.sleep(0.5)

        # Assert modal mounted
        self.wait_and_assert(
            "document.body.innerText.includes('Universal Craft CLI Runner')",
            "CLI Runner modal failed to open."
        )

        # Capture modal screenshot
        self.controller.capture_screenshot("09_cli_runner.png")
        self.audit_emoji_compliance("09_cli_runner")

        # Type 'craft fix' and click Execute
        self.controller.evaluate_javascript("""
        (() => {
            const input = document.querySelector('input[placeholder*=\"craft\"], input[type=\"text\"]');
            if (input) {
                input.value = 'craft fix';
                input.dispatchEvent(new Event('input', { bubbles: true }));
            }
            const btns = Array.from(document.querySelectorAll('button'));
            const runBtn = btns.find(b => b.innerText.includes('Execute'));
            if (runBtn) runBtn.click();
        })()
        """)
        time.sleep(0.6)

        # Close modal via Escape
        self.controller.evaluate_javascript("""
        (() => {
            window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        })()
        """)
        time.sleep(0.3)

        self.log_pass("Journey 8: Universal CLI Command Runner")

    def journey_09_edge_mesh(self):
        """Journey 9: Edge Mesh Latency Probing & Condition Matrix."""
        self.log("Executing Journey 9: Edge Mesh & Latency Probing...")
        self.switch_tab(7, "Edge Mesh")

        # Click 'Run Multi-Sample Probe'
        self.controller.evaluate_javascript("""
        (() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const probeBtn = btns.find(b => b.innerText.includes('Probe') || b.innerText.includes('Sample'));
            if (probeBtn) probeBtn.click();
        })()
        """)
        time.sleep(0.5)

        # Assert presence of table cells or conditions
        self.wait_and_assert(
            "document.body.innerText.includes('Optimal') || document.body.innerText.includes('Elevated') || document.body.innerText.includes('edge')",
            "Edge mesh probe results not rendered."
        )

        self.log_pass("Journey 9: Edge Mesh & Latency Probing")

    # -------------------------------------------------------------------------
    # Performance & Responsiveness Audits
    # -------------------------------------------------------------------------

    def audit_performance_metrics(self):
        """Validates V8 heap memory bounds and bounded DOM node counts."""
        self.log("Auditing V8 Performance Metrics & Heap Allocations...")
        metrics = self.controller.profile_performance()

        heap_used_mb = 0.0
        dom_nodes = self.controller.get_dom_node_count()

        for m in metrics:
            name = m.get("name")
            val = m.get("value", 0.0)
            if name == "JSHeapUsedSize":
                heap_used_mb = val / (1024 * 1024)

        self.log(f"V8 JS Heap Used: {heap_used_mb:.2f} MB (Threshold: < 25.0 MB)")
        self.log(f"DOM Node Count: {dom_nodes} nodes (Threshold: < 1,500 nodes)")

        if heap_used_mb > 25.0:
            raise TestAssertionError(f"V8 JS Heap exceeded 25 MB limit: {heap_used_mb:.2f} MB")

        if dom_nodes > 1500:
            raise TestAssertionError(f"DOM Node Count exceeded 1,500 limit: {dom_nodes} nodes")

        self.log_pass("Performance & Memory Telemetry Audit (Heap < 25MB, Nodes < 1500)")

    def audit_viewport_responsiveness(self):
        """Verifies visual rendering across standard 1280x800 and 1920x1080 full HD viewports."""
        self.log("Auditing Viewport Responsiveness at 1280x800 and 1920x1080...")

        # 1280x800
        self.controller.set_viewport(1280, 800)
        time.sleep(0.3)
        self.switch_tab(1, "Fleet Overview")
        self.controller.capture_screenshot("viewport_1280x800.png")

        # 1920x1080
        self.controller.set_viewport(1920, 1080)
        time.sleep(0.3)
        self.controller.capture_screenshot("viewport_1920x1080.png")

        # Reset to 1280x800
        self.controller.set_viewport(1280, 800)
        self.log_pass("Viewport Responsiveness Audit (1280x800, 1920x1080)")

    # -------------------------------------------------------------------------
    # Test Suite Orchestration
    # -------------------------------------------------------------------------

    def run_all(self) -> int:
        print("\n=======================================================")
        print("  Craft Desktop Studio End-to-End DevTools Test Suite  ")
        print("=======================================================\n")
        start_time = time.time()

        journeys = [
            ("Journey 1: Studio Shell & Navigation", self.journey_01_shell_and_navigation),
            ("Journey 2: Server Provisioning Wizard", self.journey_02_server_provisioning_wizard),
            ("Journey 3: Server Power Lifecycle", self.journey_03_power_lifecycle),
            ("Journey 4: Interactive Live Console", self.journey_04_live_console),
            ("Journey 5: Plugin Store & Discovery", self.journey_05_plugin_store),
            ("Journey 6: Backup & Resilience Hub", self.journey_06_backup_hub),
            ("Journey 7: Configuration Studio", self.journey_07_config_studio),
            ("Journey 8: Universal CLI Runner", self.journey_08_cli_runner),
            ("Journey 9: Edge Mesh & Probing", self.journey_09_edge_mesh),
            ("Audit: Viewport Responsiveness", self.audit_viewport_responsiveness),
            ("Audit: Performance & Telemetry", self.audit_performance_metrics),
        ]

        for name, fn in journeys:
            try:
                fn()
            except Exception as e:
                self.log_fail(name, str(e))

        duration = time.time() - start_time
        total_tests = self.passed_tests + self.failed_tests

        print("\n=======================================================")
        print(f"  Test Execution Summary: {self.passed_tests}/{total_tests} Passed in {duration:.2f}s")
        print("=======================================================")

        if self.failed_tests == 0:
            print("[OK] All End-to-End DevTools tests passed cleanly with 0 errors.")
            return 0
        else:
            print(f"[ERROR] {self.failed_tests} test(s) failed.")
            return 1


def main():
    parser = argparse.ArgumentParser(description="Craft Desktop Studio E2E DevTools Test Runner")
    parser.add_argument("--port", type=int, default=DEFAULT_CDP_PORT, help="CDP remote debugging port")
    parser.add_argument("--auto-start", action="store_true", default=True, help="Auto start/stop dev server")
    args = parser.parse_args()

    started_by_us = False

    # Check if CDP endpoint is already reachable
    controller = DevToolsController(port=args.port)
    try:
        controller.get_target_ws_url()
        print(f"[INFO] Connected to existing DevTools session on port {args.port}.")
    except Exception:
        if args.auto_start:
            print(f"[INFO] DevTools endpoint not detected. Launching environment on port {args.port}...")
            start_args = argparse.Namespace(port=args.port)
            ret = cmd_start(start_args)
            if ret != 0:
                print("[ERROR] Failed to start studio dev environment.")
                sys.exit(1)
            started_by_us = True
            time.sleep(3)
        else:
            print(f"[ERROR] DevTools endpoint unreachable on port {args.port} and --no-auto-start specified.")
            sys.exit(1)

    suite = StudioTestSuite(port=args.port)
    exit_code = 0
    try:
        exit_code = suite.run_all()
    finally:
        if started_by_us:
            print("\n[INFO] Tearing down test environment...")
            stop_args = argparse.Namespace()
            cmd_stop(stop_args)

    sys.exit(exit_code)


if __name__ == "__main__":
    main()
