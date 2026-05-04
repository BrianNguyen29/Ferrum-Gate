#!/usr/bin/env python3
"""
Target Evidence Capture Script for FerrumGate v1.

This script collects readiness and metrics outputs from a running FerrumGate
server into a timestamped directory for evidence collection and analysis.

IMPORTANT: This script does NOT complete G2/operator signoff and does NOT
authorize pilot. It only collects evidence outputs. Operator review and
explicit signoff is still required per the G2 gates in the implementation path.

Usage:
    python3 scripts/capture_target_evidence.py [--server-url URL] [--bearer-token TOKEN] [--output-dir DIR]
"""

import argparse
import datetime
import json
import os
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FERRUMCTL = os.environ.get("FERRUMCTL", "ferrumctl")


def sanitize_token(text: str, token: str) -> str:
    """Replace Authorization/Bearer tokens with [REDACTED]."""
    if not token:
        return text
    return text.replace(token, "[REDACTED]")


def run_command(cmd, description, timeout=30):
    """Run a command and return (success, stdout, stderr)."""
    print(f"  Capturing: {description}")
    try:
        result = subprocess.run(
            cmd,
            shell=False,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode == 0, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return False, "", f"Command timed out after {timeout}s"
    except Exception as e:
        return False, "", str(e)


def capture_shallow_readiness(server_url, bearer_token):
    """Capture shallow readiness probe output."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "server",
        "readiness",
    ]
    success, stdout, stderr = run_command(cmd, "Shallow Readiness (/v1/readyz)")
    return {
        "endpoint": "/v1/readyz",
        "success": success,
        "output": sanitize_token(stdout + stderr, bearer_token),
    }


def capture_deep_readiness(server_url, bearer_token):
    """Capture deep readiness probe output."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "server",
        "readiness",
        "--deep",
    ]
    success, stdout, stderr = run_command(cmd, "Deep Readiness (/v1/readyz/deep)")
    return {
        "endpoint": "/v1/readyz/deep",
        "success": success,
        "output": sanitize_token(stdout + stderr, bearer_token),
    }


def capture_metrics(server_url, bearer_token):
    """Capture metrics endpoint output."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "--bearer-token", bearer_token,
        "server",
        "metrics",
    ]
    success, stdout, stderr = run_command(cmd, "Metrics (/v1/metrics)")
    return {
        "endpoint": "/v1/metrics",
        "success": success,
        "output": sanitize_token(stdout + stderr, bearer_token),
    }


def capture_health(server_url, bearer_token):
    """Capture health endpoint output."""
    cmd = [
        FERRUMCTL,
        "--server-url", server_url,
        "server",
        "health",
    ]
    success, stdout, stderr = run_command(cmd, "Health (/v1/healthz)")
    return {
        "endpoint": "/v1/healthz",
        "success": success,
        "output": sanitize_token(stdout + stderr, bearer_token),
    }


def parse_args():
    parser = argparse.ArgumentParser(
        description="Target evidence capture for FerrumGate v1.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
IMPORTANT: This script does NOT complete G2/operator signoff.

It only collects evidence outputs (readiness, metrics) into a timestamped
directory. Operator review and explicit signoff is still required.

The script sanitizes Authorization/Bearer tokens in the captured output
by replacing them with [REDACTED].
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
        "--output-dir",
        default=None,
        help="Output directory (default: ./evidence-{timestamp})",
    )
    return parser.parse_args()


def main():
    print("=" * 60)
    print("FerrumGate v1 Target Evidence Capture")
    print("=" * 60)
    print()
    print("IMPORTANT: This script does NOT complete G2/operator signoff.")
    print("It only collects evidence outputs. Operator review is still required.")
    print()

    args = parse_args()
    server_url = args.server_url
    bearer_token = args.bearer_token

    # Create timestamped output directory
    if args.output_dir:
        output_dir = args.output_dir
    else:
        timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
        output_dir = f"./evidence-{timestamp}"

    os.makedirs(output_dir, exist_ok=True)
    print(f"Output directory: {output_dir}")
    print()

    # Collect evidence
    print("Collecting evidence...")
    print("-" * 40)

    evidence = {
        "collection_time": datetime.datetime.now().isoformat(),
        "server_url": server_url,
        "artifacts": [],
    }

    # Capture shallow readiness
    artifact = capture_shallow_readiness(server_url, bearer_token)
    evidence["artifacts"].append(artifact)
    status = "OK" if artifact["success"] else "FAIL"
    print(f"  [{status}] Shallow Readiness")

    # Capture deep readiness
    artifact = capture_deep_readiness(server_url, bearer_token)
    evidence["artifacts"].append(artifact)
    status = "OK" if artifact["success"] else "FAIL"
    print(f"  [{status}] Deep Readiness")

    # Capture metrics
    artifact = capture_metrics(server_url, bearer_token)
    evidence["artifacts"].append(artifact)
    status = "OK" if artifact["success"] else "FAIL"
    print(f"  [{status}] Metrics")

    # Capture health
    artifact = capture_health(server_url, bearer_token)
    evidence["artifacts"].append(artifact)
    status = "OK" if artifact["success"] else "FAIL"
    print(f"  [{status}] Health")

    print("-" * 40)
    print()

    # Write evidence files
    print(f"Writing evidence files to {output_dir}...")

    # Write main evidence JSON
    evidence_path = os.path.join(output_dir, "evidence.json")
    with open(evidence_path, "w") as f:
        json.dump(evidence, f, indent=2)
    print(f"  Written: {evidence_path}")

    # Write individual artifact files
    for i, artifact in enumerate(evidence["artifacts"]):
        endpoint_name = artifact["endpoint"].replace("/", "_").strip("_")
        artifact_path = os.path.join(output_dir, f"{i:02d}_{endpoint_name}.txt")
        with open(artifact_path, "w") as f:
            f.write(artifact["output"])
        print(f"  Written: {artifact_path}")

    # Write README
    readme_path = os.path.join(output_dir, "README.txt")
    readme_content = """FerrumGate v1 Target Evidence Capture
=====================================

This directory contains evidence outputs collected from a FerrumGate server.

IMPORTANT NOTICE
---------------
This evidence collection does NOT complete G2/operator signoff.
It does NOT authorize pilot or indicate production readiness.

Operator review and explicit signoff is still required per the G2 gates
in the implementation path documentation.

Collected Artifacts
------------------
- evidence.json: Main evidence file with all captured data
- 00_healthz.txt: Health endpoint output
- 01_readyz.txt: Shallow readiness endpoint output
- 02_readyz_deep.txt: Deep readiness endpoint output
- 03_metrics.txt: Prometheus metrics endpoint output

Sanitization
------------
Authorization/Bearer tokens in the captured output have been replaced
with [REDACTED] to avoid credential exposure.

Collection Time
--------------
{collection_time}

Server URL
----------
{server_url}
""".format(
        collection_time=evidence["collection_time"],
        server_url=server_url,
    )
    with open(readme_path, "w") as f:
        f.write(readme_content)
    print(f"  Written: {readme_path}")

    print()
    print("Evidence collection complete.")
    print()
    print("IMPORTANT: This evidence collection does NOT complete G2/operator signoff.")
    print("           Operator review and explicit signoff is still required.")
    print()

    return 0


if __name__ == "__main__":
    sys.exit(main())
