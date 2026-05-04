#!/usr/bin/env python3
"""
Pilot Readiness Check Script for FerrumGate v1.

This script runs shallow, deep, and functional readiness probes via ferrumctl
or HTTP to provide a quick pass/fail status report for pilot readiness evaluation.

IMPORTANT: This script does NOT complete G2/operator signoff. It only runs
automated probes and reports their results. Operator review and explicit
signoff is still required per the G2 gates in docs/implementation-path/59-pilot-readiness-evidence-packet.md.

Usage:
    python3 scripts/check_pilot_readiness.py [--server-url URL] [--bearer-token TOKEN]
"""

import argparse
import subprocess
import sys
import os
import json
import re

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FERRUMCTL = os.environ.get("FERRUMCTL", "ferrumctl")


def run_command(cmd, description, timeout=30, capture_json=False):
    """Run a command and return (success, output)."""
    print(f"\n{'=' * 60}")
    print(f"Probe: {description}")
    print(f"Command: {' '.join(cmd)}")
    print(f"{'=' * 60}")
    try:
        result = subprocess.run(
            cmd,
            shell=False,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        success = result.returncode == 0
        output = result.stdout + result.stderr

        if capture_json and success:
            try:
                # Extract JSON from output (in case there's extra stderr)
                parsed = json.loads(result.stdout.strip())
                output = json.dumps(parsed, indent=2)
            except json.JSONDecodeError:
                pass

        print(f"Result: {'PASS' if success else 'FAIL'}")
        if not success:
            print(f"Output (last 500 chars):\n{output[-500:]}")
        return success, output
    except subprocess.TimeoutExpired:
        print(f"Result: TIMEOUT (>{timeout}s)")
        return False, f"Command timed out after {timeout}s"
    except Exception as e:
        print(f"Result: ERROR - {e}")
        return False, str(e)


def check_shallow_readiness(server_url, bearer_token):
    """Shallow readiness probe: GET /v1/readyz (no auth required)."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "server",
        "readiness",
    ]
    # Note: no --deep or --functional flags = shallow probe
    return run_command(cmd, "Shallow Readiness (/v1/readyz)", capture_json=True)


def check_deep_readiness(server_url, bearer_token):
    """Deep readiness probe: GET /v1/readyz/deep (no auth required)."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "server",
        "readiness",
        "--deep",
    ]
    success, output = run_command(cmd, "Deep Readiness (/v1/readyz/deep)", capture_json=True)

    # Attempt to parse JSON body and verify components if safe
    # Skip if output format is unpredictable or parsing failed
    if success:
        try:
            # Try to extract JSON from output (output may contain stderr/stdout mix)
            json_match = re.search(r'\{.*\}', output, re.DOTALL)
            if json_match:
                body = json.loads(json_match.group())
                if "components" in body and isinstance(body["components"], list):
                    component_names = {c.get("component") for c in body["components"] if isinstance(c, dict)}
                    has_store = "store" in component_names
                    has_write_queue = "write_queue" in component_names

                    if has_store:
                        print("  -> Found component: store")
                    else:
                        print("  -> MISSING component: store")

                    if has_write_queue:
                        print("  -> Found component: write_queue")
                    else:
                        print("  -> MISSING component: write_queue")

                    if not (has_store and has_write_queue):
                        print("  -> FAIL: Missing required components (store or write_queue)")
                        success = False
                else:
                    print("  -> Skipped body parsing: components field not found or not an array")
            else:
                print("  -> Skipped body parsing: no JSON object found in output")
        except (json.JSONDecodeError, KeyError, TypeError) as e:
            print(f"  -> Skipped body parsing: {type(e).__name__} — output format may be unsafe")
    return success, output


def check_functional_readiness(server_url, bearer_token):
    """Functional readiness probe: GET /v1/approvals?limit=1 (auth required)."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "--bearer-token", bearer_token,
        "server",
        "readiness",
        "--functional",
    ]
    return run_command(cmd, "Functional Readiness (/v1/approvals?limit=1)", capture_json=True)


def check_metrics_endpoint(server_url, bearer_token):
    """Check /v1/metrics endpoint is accessible and contains required metrics."""
    # Use curl directly since ferrumctl doesn't have a metrics subcommand
    auth_header = f"Bearer {bearer_token}" if bearer_token else ""
    if auth_header:
        cmd = [
            "curl",
            "-s",
            "-w", "\\n%{http_code}",
            f"{server_url}/v1/metrics",
            "-H", f"Authorization: {auth_header}",
        ]
    else:
        cmd = [
            "curl",
            "-s",
            "-w", "\\n%{http_code}",
            f"{server_url}/v1/metrics",
        ]
    success, output = run_command(cmd, "Metrics Endpoint (/v1/metrics)", capture_json=False)
    if success:
        # Check for required metrics
        has_write_queue_depth = "ferrumgate_write_queue_depth" in output
        has_method_label = 'method="GET"' in output
        has_http_requests = "ferrumgate_http_requests_total" in output
        has_store_health = "ferrumgate_store_health_up" in output

        if has_write_queue_depth:
            print("  -> Found: ferrumgate_write_queue_depth")
        else:
            print("  -> MISSING: ferrumgate_write_queue_depth")

        if has_method_label:
            print("  -> Found: method=\"GET\" label")
        else:
            print("  -> MISSING: method=\"GET\" label")

        if has_http_requests:
            print("  -> Found: ferrumgate_http_requests_total")

        if has_store_health:
            print("  -> Found: ferrumgate_store_health_up")

        # Require ferrumgate_write_queue_depth AND method="GET" label per pilot readiness spec
        required_metrics_present = has_write_queue_depth and has_method_label
        if not required_metrics_present:
            print("  -> FAIL: Missing required metrics (write_queue_depth or method label)")
            success = False
    return success, output


def check_backup_verify(db_path):
    """Check that backup verify command works (local, no server required)."""
    cmd = [
        FERRUMCTL,
        "backup",
        "verify",
        "--db-path", str(db_path),
    ]
    return run_command(cmd, f"Backup Verify ({db_path})")


def parse_args():
    parser = argparse.ArgumentParser(
        description="Pilot readiness check for FerrumGate v1.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
IMPORTANT: This script does NOT complete G2/operator signoff.

It only runs automated probes and reports their results. Operator review
and explicit signoff is still required per the G2 gates in:
  docs/implementation-path/59-pilot-readiness-evidence-packet.md

G2 gates require operator acknowledgment and, where indicated,
documented accepted-risk signoff. Do not mark G2 items complete
on behalf of the operator.
        """,
    )
    parser.add_argument(
        "--server-url",
        default=os.environ.get("FERRUMCTL_SERVER_URL", "http://127.0.0.1:8080"),
        help="FerrumGate server URL (default: http://127.0.0.1:8080)",
    )
    parser.add_argument(
        "--bearer-token",
        default=os.environ.get("FERRUMCTL_BEARER_TOKEN", ""),
        help="Bearer token for authenticated endpoints",
    )
    parser.add_argument(
        "--skip-metrics",
        action="store_true",
        help="Skip metrics endpoint check",
    )
    parser.add_argument(
        "--skip-functional",
        action="store_true",
        help="Skip functional readiness probe (requires server running with auth)",
    )
    return parser.parse_args()


def main():
    print("=" * 60)
    print("FerrumGate v1 Pilot Readiness Check")
    print("=" * 60)
    print()
    print("IMPORTANT: This script does NOT complete G2/operator signoff.")
    print("It only runs automated probes. Operator review is still required.")
    print()

    args = parse_args()
    server_url = args.server_url
    bearer_token = args.bearer_token

    results = {}

    # Run probes
    print("\n" + "=" * 60)
    print("RUNNING READINESS PROBES")
    print("=" * 60)

    # Shallow readiness
    success, _ = check_shallow_readiness(server_url, bearer_token)
    results["shallow_readiness"] = success

    # Deep readiness
    success, _ = check_deep_readiness(server_url, bearer_token)
    results["deep_readiness"] = success

    # Functional readiness (if not skipped)
    if not args.skip_functional:
        success, _ = check_functional_readiness(server_url, bearer_token)
        results["functional_readiness"] = success
    else:
        print("\n[SKIPPED] Functional readiness probe (--skip-functional)")
        results["functional_readiness"] = None

    # Metrics endpoint (if not skipped)
    if not args.skip_metrics:
        success, _ = check_metrics_endpoint(server_url, bearer_token)
        results["metrics_endpoint"] = success
    else:
        print("\n[SKIPPED] Metrics endpoint check (--skip-metrics)")
        results["metrics_endpoint"] = None

    # Summary
    print("\n" + "=" * 60)
    print("PILOT READINESS SUMMARY")
    print("=" * 60)
    for probe, status in results.items():
        if status is None:
            display_status = "SKIPPED"
        elif status:
            display_status = "PASS"
        else:
            display_status = "FAIL"
        print(f"  {probe}: {display_status}")
    print("=" * 60)

    # Overall result
    active_results = [v for v in results.values() if v is not None]
    all_passed = all(active_results) if active_results else False
    any_failed = any(not v for v in active_results) if active_results else False

    print()
    if all_passed:
        print("Overall: ALL PROBES PASSED")
    elif any_failed:
        print("Overall: SOME PROBES FAILED")
    else:
        print("Overall: NO PROBES RUN (all skipped)")

    print()
    print("NOTE: This automated check does NOT complete G2/operator signoff.")
    print("      Operator review and explicit signoff is still required.")
    print()
    print("See: docs/implementation-path/59-pilot-readiness-evidence-packet.md")
    print()

    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
