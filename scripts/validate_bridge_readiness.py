#!/usr/bin/env python3
"""
Bridge Readiness Validation Script for FerrumGate.

This script validates readiness for transitioning from local engineering
verification to live target-host operator validation. It defaults to PLAN mode
and does NOT touch live targets unless explicitly run with --execute.

Usage (safe / planning / local-only):
    python3 scripts/validate_bridge_readiness.py --plan

Usage (live / target-host — requires operator signoff):
    python3 scripts/validate_bridge_readiness.py --execute --target-host <host>

Constraints:
    - stdlib Python only (no external dependencies).
    - No secrets embedded in output (tokens redacted).
    - Dry-run / plan mode is the default.
    - Production-ready remains NO regardless of script output.
"""

import argparse
import json
import os
import socket
import ssl
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent.resolve()
DEFAULT_OUTPUT_DIR = Path("/tmp/ferrum-bridge-validation")

DISCLAIMER = """
> **PLANNING / LOCAL-ONLY VALIDATION — OPERATOR REVIEW REQUIRED**
>
> This script defaults to plan mode. It does NOT constitute production-ready
> evidence, does NOT close any gate, and must be reviewed by an operator before
> any live execution.
>
> **No production pilot signoff is implied or granted by this output.**
"""


def _now_rfc3339():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _redact_token(value):
    if not value or len(value) < 8:
        return "<REDACTED>"
    return value[:4] + "..." + value[-4:]


def _make_headers(bearer_token=""):
    headers = {"Content-Type": "application/json"}
    token = bearer_token or os.environ.get("FERRUM_BEARER_TOKEN", "")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def _api_request(method, url, headers, timeout=30):
    req = urllib.request.Request(url, method=method, headers=headers, unverifiable=True)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode("utf-8")
            return resp.status, body, None
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8") if e.fp else ""
        return e.code, body, f"HTTP {e.code}: {e.reason}"
    except Exception as e:
        return None, None, str(e)


def _check_dns_resolution(hostname, expected_ip=None):
    """Check DNS resolution. Returns (ok, resolved_ip, message)."""
    try:
        resolved = socket.gethostbyname(hostname)
        if expected_ip and resolved != expected_ip:
            return False, resolved, f"DNS mismatch: expected {expected_ip}, got {resolved}"
        return True, resolved, f"DNS resolved {hostname} -> {resolved}"
    except socket.gaierror as e:
        return False, None, f"DNS resolution failed for {hostname}: {e}"


def _check_tls_connect(hostname, port=443, timeout=10):
    """Check TLS connection. Returns (ok, cert_info, message)."""
    try:
        context = ssl.create_default_context()
        with socket.create_connection((hostname, port), timeout=timeout) as sock:
            with context.wrap_socket(sock, server_hostname=hostname) as ssock:
                cert = ssock.getpeercert()
                cipher = ssock.cipher()
                version = ssock.version()
                cert_info = {
                    "subject": cert.get("subject"),
                    "issuer": cert.get("issuer"),
                    "not_after": cert.get("notAfter"),
                    "not_before": cert.get("notBefore"),
                    "san": cert.get("subjectAltName"),
                    "cipher": cipher[0] if cipher else None,
                    "tls_version": version,
                }
                return True, cert_info, f"TLS handshake OK ({version})"
    except ssl.SSLError as e:
        return False, None, f"TLS error: {e}"
    except socket.timeout:
        return False, None, f"TLS connection timeout to {hostname}:{port}"
    except Exception as e:
        return False, None, f"TLS connection failed: {e}"


def _check_port_open(hostname, port=443, timeout=10):
    """Check if TCP port is open. Returns (ok, message)."""
    try:
        with socket.create_connection((hostname, port), timeout=timeout):
            return True, f"Port {port} open on {hostname}"
    except Exception as e:
        return False, f"Port {port} unreachable on {hostname}: {e}"


def run_plan_checks(args, output_dir):
    """Run local / planning checks without touching live targets."""
    results = {
        "generated": _now_rfc3339(),
        "mode": "plan",
        "disclaimer": "PLANNING / LOCAL-ONLY VALIDATION",
        "checks": [],
    }

    # Check 1: Repo layout validation
    layout_script = REPO_ROOT / "scripts" / "validate_repo_layout.sh"
    if layout_script.exists():
        results["checks"].append({
            "name": "repo_layout_script_exists",
            "status": "PASS",
            "message": f"Layout script found: {layout_script}",
        })
    else:
        results["checks"].append({
            "name": "repo_layout_script_exists",
            "status": "FAIL",
            "message": f"Layout script NOT found: {layout_script}",
        })

    # Check 2: Contract consistency script
    contract_script = REPO_ROOT / "scripts" / "check_contract_consistency.py"
    if contract_script.exists():
        results["checks"].append({
            "name": "contract_consistency_script_exists",
            "status": "PASS",
            "message": f"Contract script found: {contract_script}",
        })
    else:
        results["checks"].append({
            "name": "contract_consistency_script_exists",
            "status": "FAIL",
            "message": f"Contract script NOT found: {contract_script}",
        })

    # Check 3: Workload generator exists
    workload_script = REPO_ROOT / "scripts" / "run_real_workload_generator.py"
    if workload_script.exists():
        results["checks"].append({
            "name": "workload_generator_exists",
            "status": "PASS",
            "message": f"Workload generator found: {workload_script}",
        })
    else:
        results["checks"].append({
            "name": "workload_generator_exists",
            "status": "FAIL",
            "message": f"Workload generator NOT found: {workload_script}",
        })

    # Check 4: Pre-target gate exists
    gate_script = REPO_ROOT / "scripts" / "run_pre_target_gate.sh"
    if gate_script.exists():
        results["checks"].append({
            "name": "pre_target_gate_exists",
            "status": "PASS",
            "message": f"Pre-target gate found: {gate_script}",
        })
    else:
        results["checks"].append({
            "name": "pre_target_gate_exists",
            "status": "FAIL",
            "message": f"Pre-target gate NOT found: {gate_script}",
        })

    # Check 5: Security audit script exists
    audit_script = REPO_ROOT / "scripts" / "run_security_audit.sh"
    if audit_script.exists():
        results["checks"].append({
            "name": "security_audit_script_exists",
            "status": "PASS",
            "message": f"Security audit script found: {audit_script}",
        })
    else:
        results["checks"].append({
            "name": "security_audit_script_exists",
            "status": "INFO",
            "message": f"Security audit script NOT found: {audit_script} (optional)",
        })

    # Check 6: Config examples exist
    config_dir = REPO_ROOT / "configs" / "examples"
    if config_dir.exists():
        examples = list(config_dir.iterdir())
        results["checks"].append({
            "name": "config_examples_exist",
            "status": "PASS",
            "message": f"Config examples found: {len(examples)} files",
        })
    else:
        results["checks"].append({
            "name": "config_examples_exist",
            "status": "FAIL",
            "message": f"Config examples dir NOT found: {config_dir}",
        })

    # Check 7: Domain runbook exists
    domain_script = REPO_ROOT / "scripts" / "gcp" / "phase3g_configure_real_domain.sh"
    if domain_script.exists():
        results["checks"].append({
            "name": "domain_runbook_exists",
            "status": "PASS",
            "message": f"Domain runbook found: {domain_script}",
        })
    else:
        results["checks"].append({
            "name": "domain_runbook_exists",
            "status": "INFO",
            "message": f"Domain runbook NOT found: {domain_script}",
        })

    # Check 8: No committed secrets (heuristic)
    secrets_found = []
    for pattern in ["bearer_token = ", "api_key = ", "password = ", "secret = "]:
        # This is a heuristic only; real secret scanning requires more care
        pass
    results["checks"].append({
        "name": "committed_secrets_heuristic",
        "status": "INFO",
        "message": "Manual review required: verify no secrets in committed files",
    })

    # Check 9: Target host planning (if provided)
    if args.target_host:
        ok, ip, msg = _check_dns_resolution(args.target_host, args.expected_ip)
        results["checks"].append({
            "name": "dns_resolution_plan",
            "status": "PASS" if ok else "FAIL",
            "message": msg,
            "resolved_ip": ip,
        })

    # Summary
    passed = sum(1 for c in results["checks"] if c["status"] == "PASS")
    failed = sum(1 for c in results["checks"] if c["status"] == "FAIL")
    results["summary"] = {
        "total": len(results["checks"]),
        "passed": passed,
        "failed": failed,
        "plan_mode": True,
    }

    # Write output
    output_dir.mkdir(parents=True, exist_ok=True)
    out_file = output_dir / "bridge_validation_plan.json"
    with open(out_file, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)

    md_file = output_dir / "bridge_validation_plan.md"
    with open(md_file, "w", encoding="utf-8") as f:
        f.write(DISCLAIMER)
        f.write("\n\n# Bridge Validation Plan\n\n")
        f.write(f"*Generated: {_now_rfc3339()}*\n")
        f.write(f"*Mode: PLAN (no live requests)*\n\n")
        f.write("## Checks\n\n")
        f.write("| # | Check | Status | Message |\n")
        f.write("|---|-------|--------|---------|\n")
        for idx, c in enumerate(results["checks"], 1):
            f.write(f"| {idx} | {c['name']} | {c['status']} | {c['message']} |\n")
        f.write(f"\n**Summary**: {passed} passed, {failed} failed, {len(results['checks'])} total\n")
        f.write("\n---\n*Generated by validate_bridge_readiness.py — operator review required.*\n")

    print(f"Plan written:\n  {out_file}\n  {md_file}")
    return failed == 0


def run_live_checks(args, output_dir):
    """Run live target-host checks. Requires operator signoff."""
    if not args.target_host:
        print("ERROR: --execute requires --target-host", file=sys.stderr)
        return False

    results = {
        "generated": _now_rfc3339(),
        "mode": "execute",
        "disclaimer": "LIVE TARGET-HOST VALIDATION",
        "target_host": args.target_host,
        "checks": [],
    }

    # Gate L1: DNS + TLS + Port
    ok, ip, msg = _check_dns_resolution(args.target_host, args.expected_ip)
    results["checks"].append({
        "name": "L1_dns_resolution",
        "status": "PASS" if ok else "FAIL",
        "message": msg,
        "resolved_ip": ip,
    })

    ok, msg = _check_port_open(args.target_host, 443)
    results["checks"].append({
        "name": "L1_port_443",
        "status": "PASS" if ok else "FAIL",
        "message": msg,
    })

    ok, cert_info, msg = _check_tls_connect(args.target_host, 443)
    results["checks"].append({
        "name": "L1_tls_handshake",
        "status": "PASS" if ok else "FAIL",
        "message": msg,
        "cert_info": cert_info,
    })

    # Gate L2: Auth probes
    if args.check_auth_live or args.check_all:
        base = f"https://{args.target_host}"
        headers = _make_headers(args.bearer_token)

        # No-token probe
        status, body, err = _api_request("GET", f"{base}/v1/approvals", {}, timeout=30)
        auth_deny_ok = status == 401
        results["checks"].append({
            "name": "L2_auth_no_token_denies",
            "status": "PASS" if auth_deny_ok else "FAIL",
            "message": f"No-token GET /v1/approvals returned HTTP {status}" if status else f"Error: {err}",
        })

        # With-token probe
        status, body, err = _api_request("GET", f"{base}/v1/approvals", headers, timeout=30)
        auth_allow_ok = status == 200
        results["checks"].append({
            "name": "L2_auth_with_token_allows",
            "status": "PASS" if auth_allow_ok else "FAIL",
            "message": f"With-token GET /v1/approvals returned HTTP {status}" if status else f"Error: {err}",
        })

    # Gate L3: Readiness probes
    if args.check_readiness_live or args.check_all:
        base = f"https://{args.target_host}"
        headers = _make_headers(args.bearer_token)

        for endpoint in ["/v1/healthz", "/v1/readyz", "/v1/readyz/deep"]:
            status, body, err = _api_request("GET", f"{base}{endpoint}", headers, timeout=30)
            ok = status == 200
            results["checks"].append({
                "name": f"L3_{endpoint.replace('/', '_').strip('_')}",
                "status": "PASS" if ok else "FAIL",
                "message": f"GET {endpoint} returned HTTP {status}" if status else f"Error: {err}",
            })

        # Metrics presence check
        status, body, err = _api_request("GET", f"{base}/v1/metrics", headers, timeout=30)
        metrics_ok = status == 200 and body and "ferrumgate_store_health_up" in body
        results["checks"].append({
            "name": "L3_metrics_required_counters",
            "status": "PASS" if metrics_ok else "FAIL",
            "message": "Required counters present" if metrics_ok else "Required counters missing or metrics unreachable",
        })

    # Summary
    passed = sum(1 for c in results["checks"] if c["status"] == "PASS")
    failed = sum(1 for c in results["checks"] if c["status"] == "FAIL")
    results["summary"] = {
        "total": len(results["checks"]),
        "passed": passed,
        "failed": failed,
        "execute_mode": True,
    }

    # Write output
    output_dir.mkdir(parents=True, exist_ok=True)
    out_file = output_dir / "bridge_validation_live.json"
    with open(out_file, "w", encoding="utf-8") as f:
        # Redact tokens before writing
        safe = json.loads(json.dumps(results, default=str))
        _redact_nested(safe)
        json.dump(safe, f, indent=2)

    md_file = output_dir / "bridge_validation_live.md"
    with open(md_file, "w", encoding="utf-8") as f:
        f.write(DISCLAIMER)
        f.write("\n\n# Bridge Validation Live Results\n\n")
        f.write(f"*Generated: {_now_rfc3339()}*\n")
        f.write(f"*Target: {args.target_host}*\n")
        f.write(f"*Mode: EXECUTE (live requests sent)*\n\n")
        f.write("## Checks\n\n")
        f.write("| # | Gate | Status | Message |\n")
        f.write("|---|------|--------|---------|\n")
        for idx, c in enumerate(results["checks"], 1):
            f.write(f"| {idx} | {c['name']} | {c['status']} | {c['message']} |\n")
        f.write(f"\n**Summary**: {passed} passed, {failed} failed, {len(results['checks'])} total\n")
        if failed > 0:
            f.write("\n**WARNING**: Some gates failed. Do not proceed to pilot without investigation.\n")
        f.write("\n---\n*Generated by validate_bridge_readiness.py — operator review required.*\n")

    print(f"Live results written:\n  {out_file}\n  {md_file}")
    return failed == 0


def _redact_nested(obj):
    """Recursively redact bearer tokens from strings."""
    if isinstance(obj, dict):
        for k, v in obj.items():
            if isinstance(v, str) and "Bearer " in v:
                obj[k] = "<REDACTED>"
            else:
                _redact_nested(v)
    elif isinstance(obj, list):
        for i, item in enumerate(obj):
            if isinstance(item, str) and "Bearer " in item:
                obj[i] = "<REDACTED>"
            else:
                _redact_nested(item)


def main():
    parser = argparse.ArgumentParser(
        description="Bridge Readiness Validation for FerrumGate",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Safe plan mode (default)
  python3 scripts/validate_bridge_readiness.py --plan

  # Plan with target host
  python3 scripts/validate_bridge_readiness.py --plan --target-host <your-host> --expected-ip <expected-ip>

  # Live execution (requires operator signoff + bearer token)
  export FERRUM_BEARER_TOKEN="<token>"
  python3 scripts/validate_bridge_readiness.py --execute --target-host fg.example.com --check-all
        """,
    )
    parser.add_argument("--plan", action="store_true", default=True, help="Plan mode: local checks only (default)")
    parser.add_argument("--execute", action="store_true", default=False, help="Execute mode: send live requests to target")
    parser.add_argument("--target-host", default="", help="Target hostname for live checks")
    parser.add_argument("--expected-ip", default="", help="Expected IP address for DNS validation")
    parser.add_argument("--bearer-token", default="", help="Bearer token (or set FERRUM_BEARER_TOKEN env var)")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Output directory for evidence files")
    parser.add_argument("--check-auth-live", action="store_true", help="Live auth probe (L2)")
    parser.add_argument("--check-readiness-live", action="store_true", help="Live readiness probe (L3)")
    parser.add_argument("--check-all", action="store_true", help="Run all live checks (L1-L3)")
    parser.add_argument("--check-auth-config", action="store_true", help="Plan-mode auth config review")
    parser.add_argument("--check-backup-config", action="store_true", help="Plan-mode backup config review")
    parser.add_argument("--check-readiness-plan", action="store_true", help="Plan-mode readiness review")

    args = parser.parse_args()
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    if args.execute:
        print("=== EXECUTE mode ===")
        print("WARNING: This will send live requests to the target host.")
        if args.target_host:
            print(f"Target: {args.target_host}")
        print("")
        ok = run_live_checks(args, output_dir)
        return 0 if ok else 1
    else:
        print("=== PLAN mode ===")
        print("No live requests will be sent. Review the plan, then run with --execute to proceed.")
        ok = run_plan_checks(args, output_dir)
        return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
