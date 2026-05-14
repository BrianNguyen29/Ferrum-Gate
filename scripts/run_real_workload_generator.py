#!/usr/bin/env python3
"""
G3.6 Real Workload Generator for FerrumGate.

This script generates workload against a FerrumGate server to collect evidence
for G3.6 pilot metrics. It defaults to PLAN mode and does NOT send live requests
unless explicitly run with --execute.

Usage (safe / planning):
    python3 scripts/run_real_workload_generator.py --plan --server-url https://<host>

Usage (live / target-host execution):
    export FERRUM_BEARER_TOKEN="<token>"
    python3 scripts/run_real_workload_generator.py --execute --server-url https://<host>

Outputs:
    - workload_plan.json          # Structured plan and phase definitions
    - workload_plan.md            # Human-readable plan with curl commands
    - workload_results.json       # Live results (only in --execute mode)
    - workload_results.md         # Human-readable results (only in --execute mode)
    - readyz_probe_log.json       # readyz/deep probe records

Constraints:
    - stdlib Python only (no external dependencies).
    - No secrets embedded in output (tokens redacted).
    - All output labeled as planning/local until operator executes on target host.
"""

import argparse
import json
import math
import os
import random
import sys
import time
import urllib.error
import urllib.request
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent.resolve()
DEFAULT_OUTPUT_DIR = Path("/tmp/ferrum-g36-workload")

DISCLAIMER = """
> **PLANNING / LOCAL-ONLY EVIDENCE — OPERATOR REVIEW REQUIRED**
>
> This artifact was generated in plan or local-test mode. It does NOT constitute
> production-ready evidence, does NOT close any G3 gate, and must be reviewed by
> an operator before execution on a target host.
>
> All live-execution evidence requires operator signoff per:
>   - docs/implementation-path/116-g36-monitoring-execution-plan.md
>   - docs/implementation-path/106-g3-6-pilot-metrics-evidence-packet.md
>
> **No production pilot signoff is implied or granted by this output.**
"""

# ---------------------------------------------------------------------------
# Workload profile defaults (doc 116, §4)
# ---------------------------------------------------------------------------
DEFAULT_PHASES = [
    {"name": "baseline", "duration_sec": 600, "rate_rps": 0.0},
    {"name": "low",      "duration_sec": 600, "rate_rps": 0.1},
    {"name": "target",   "duration_sec": 1800, "rate_rps": 1.0},
    {"name": "spike",    "duration_sec": 300,  "rate_rps": 5.0},
    {"name": "cooldown", "duration_sec": 600,  "rate_rps": 0.0},
]

DEFAULT_ADAPTER_MIX = {
    "fs":        {"weight": 20, "intent_type": "FileWrite",       "tool_name": "fs_write"},
    "git":       {"weight": 20, "intent_type": "GitCommit",       "tool_name": "git_branch_create"},
    "http":      {"weight": 20, "intent_type": "HttpMutation",    "tool_name": "http_post"},
    "sqlite":    {"weight": 20, "intent_type": "SqliteMutation",  "tool_name": "sql_mutate"},
    "maildraft": {"weight": 20, "intent_type": "MailDraftCreate", "tool_name": "maildraft_create"},
}

# ---------------------------------------------------------------------------
# Intent compile payloads per adapter (mirrors run_d1_d6_drills.py templates)
# ---------------------------------------------------------------------------
ADAPTER_TEMPLATES = {
    "fs": {
        "principal_id": "00000000-0000-0000-0000-000000000001",
        "title": "G36 FS Workload",
        "goal": "Write file /tmp/ferrum_g36_fs.txt",
        "raw_inputs": [],
        "requested_resource_scope": [
            {
                "kind": "FilesystemPath",
                "path": "/tmp/ferrum_g36_fs.txt",
                "mode": "Write",
                "content_hash": None,
            }
        ],
        "metadata": {"g36": True, "adapter": "fs"},
    },
    "git": {
        "principal_id": "00000000-0000-0000-0000-000000000001",
        "title": "G36 Git Workload",
        "goal": "Create branch in /tmp/ferrum_g36_repo",
        "raw_inputs": [],
        "requested_resource_scope": [
            {
                "kind": "GitRepository",
                "repo_path": "/tmp/ferrum_g36_repo",
                "allowed_refs": ["main"],
                "mode": "Write",
            }
        ],
        "metadata": {"g36": True, "adapter": "git"},
    },
    "http": {
        "principal_id": "00000000-0000-0000-0000-000000000001",
        "title": "G36 HTTP Workload",
        "goal": "HTTP POST to test endpoint",
        "raw_inputs": [],
        "requested_resource_scope": [
            {
                "kind": "HttpEndpoint",
                "method": "Post",
                "base_url": "https://httpbin.org",
                "path_prefix": "/post",
                "mode": "Write",
            }
        ],
        "metadata": {"g36": True, "adapter": "http"},
    },
    "sqlite": {
        "principal_id": "00000000-0000-0000-0000-000000000001",
        "title": "G36 SQLite Workload",
        "goal": "SQLite DML insert",
        "raw_inputs": [],
        "requested_resource_scope": [
            {
                "kind": "SqliteDatabase",
                "db_path": "/tmp/ferrum_g36.db",
                "tables": ["g36_table"],
                "mode": "Write",
            }
        ],
        "metadata": {"g36": True, "adapter": "sqlite"},
    },
    "maildraft": {
        "principal_id": "00000000-0000-0000-0000-000000000001",
        "title": "G36 MailDraft Workload",
        "goal": "Create email draft",
        "raw_inputs": [],
        "requested_resource_scope": [
            {
                "kind": "EmailDraft",
                "recipient_allowlist": ["g36@example.com"],
                "subject_prefix_allowlist": ["G36"],
                "mode": "Write",
            }
        ],
        "metadata": {"g36": True, "adapter": "maildraft"},
    },
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _now_rfc3339():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _redact_token(value):
    if not value or len(value) < 8:
        return "<REDACTED>"
    return value[:4] + "..." + value[-4:]


def _make_headers(bearer_token):
    headers = {"Content-Type": "application/json"}
    token = bearer_token or os.environ.get("FERRUM_BEARER_TOKEN", "")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def _make_api_request(method, url, headers, payload=None, timeout=30):
    """Make an API request using urllib. Return (status, body, error)."""
    data = None
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, method=method, data=data, headers=headers, unverifiable=True)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode("utf-8")
            try:
                return resp.status, json.loads(body), None
            except json.JSONDecodeError:
                return resp.status, body, None
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8") if e.fp else ""
        try:
            return e.code, json.loads(body), f"HTTP {e.code}: {e.reason}"
        except json.JSONDecodeError:
            return e.code, body, f"HTTP {e.code}: {e.reason}"
    except Exception as e:
        return None, None, str(e)


def _parse_adapter_mix(mix_arg):
    """Parse --adapter-mix JSON string into dict."""
    if not mix_arg:
        return DEFAULT_ADAPTER_MIX
    parsed = json.loads(mix_arg)
    # Validate structure
    for key, val in parsed.items():
        if "weight" not in val:
            raise ValueError(f"Adapter mix entry '{key}' missing 'weight'")
    return parsed


def _parse_phases(phases_arg):
    """Parse --phases JSON string into list."""
    if not phases_arg:
        return DEFAULT_PHASES
    parsed = json.loads(phases_arg)
    if not isinstance(parsed, list):
        raise ValueError("--phases must be a JSON list")
    for p in parsed:
        for field in ("name", "duration_sec", "rate_rps"):
            if field not in p:
                raise ValueError(f"Phase missing required field: {field}")
    return parsed


def _weighted_choice(adapter_mix):
    """Pick an adapter key based on weights."""
    total = sum(v["weight"] for v in adapter_mix.values())
    r = random.uniform(0, total)
    upto = 0.0
    for key, val in adapter_mix.items():
        upto += val["weight"]
        if upto >= r:
            return key
    return list(adapter_mix.keys())[-1]


def _percentile(values, p):
    """Compute percentile of a list of numbers."""
    if not values:
        return 0.0
    s = sorted(values)
    k = (len(s) - 1) * (p / 100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return s[int(k)]
    return s[int(f)] * (c - k) + s[int(c)] * (k - f)


def _normalize_intent_compile_payload(payload):
    """Return a copy of the intent compile payload with required defaults applied."""
    normalized = dict(payload)
    if "trusted_context" not in normalized:
        normalized["trusted_context"] = {}
    return normalized


# ---------------------------------------------------------------------------
# Plan generation
# ---------------------------------------------------------------------------

def generate_plan(server_url, phases, adapter_mix, output_dir):
    """Generate a workload plan (no live requests)."""
    plan = {
        "generated": _now_rfc3339(),
        "mode": "plan",
        "server_url": server_url,
        "disclaimer": "PLANNING / LOCAL-ONLY EVIDENCE — OPERATOR REVIEW REQUIRED",
        "phases": [],
        "adapter_mix": {},
        "total_requests_estimated": 0,
    }

    total_requests = 0
    for phase in phases:
        reqs = int(phase["duration_sec"] * phase["rate_rps"])
        total_requests += reqs
        phase_plan = {
            "name": phase["name"],
            "duration_sec": phase["duration_sec"],
            "rate_rps": phase["rate_rps"],
            "estimated_requests": reqs,
            "adapter_distribution": {
                k: int(reqs * (v["weight"] / 100.0))
                for k, v in adapter_mix.items()
            },
        }
        plan["phases"].append(phase_plan)

    plan["total_requests_estimated"] = total_requests
    plan["adapter_mix"] = {
        k: {"weight": v["weight"], "intent_type": v["intent_type"], "tool_name": v["tool_name"]}
        for k, v in adapter_mix.items()
    }

    # Write JSON plan
    plan_file = output_dir / "workload_plan.json"
    with open(plan_file, "w", encoding="utf-8") as f:
        json.dump(plan, f, indent=2)

    # Write Markdown plan
    md_file = output_dir / "workload_plan.md"
    with open(md_file, "w", encoding="utf-8") as f:
        f.write(DISCLAIMER)
        f.write("\n\n# G3.6 Workload Plan\n\n")
        f.write(f"*Generated: {_now_rfc3339()}*\n")
        f.write(f"*Server: {server_url}*\n")
        f.write(f"*Mode: PLAN (no live requests)*\n\n")
        f.write("## Phases\n\n")
        f.write("| Phase | Duration (s) | Rate (rps) | Est. Requests |\n")
        f.write("|-------|-------------:|-----------:|--------------:|\n")
        for p in plan["phases"]:
            f.write(f"| {p['name']} | {p['duration_sec']} | {p['rate_rps']} | {p['estimated_requests']} |\n")
        f.write(f"\n**Total estimated requests**: {total_requests}\n\n")
        f.write("## Adapter Mix\n\n")
        f.write("| Adapter | Weight | Intent Type | Tool Name |\n")
        f.write("|---------|--------|-------------|-----------|\n")
        for k, v in plan["adapter_mix"].items():
            f.write(f"| {k} | {v['weight']} | {v['intent_type']} | {v['tool_name']} |\n")
        f.write("\n## Sample Intent Compile Payloads\n\n")
        for adapter_key in adapter_mix:
            template = _normalize_intent_compile_payload(ADAPTER_TEMPLATES.get(adapter_key, {}))
            f.write(f"### {adapter_key.upper()}\n\n")
            f.write("```json\n")
            f.write(json.dumps(template, indent=2))
            f.write("\n```\n\n")
        f.write("\n---\n*Generated by run_real_workload_generator.py — operator review required.*\n")

    return str(plan_file), str(md_file), plan


# ---------------------------------------------------------------------------
# Live execution
# ---------------------------------------------------------------------------

def _execute_single_request(server_url, adapter_key, headers):
    """Execute a single intent-compile request for the given adapter."""
    payload = _normalize_intent_compile_payload(ADAPTER_TEMPLATES.get(adapter_key, {}))
    url = f"{server_url}/v1/intents/compile"
    start = time.perf_counter()
    status, body, err = _make_api_request("POST", url, headers, payload, timeout=30)
    elapsed_ms = round((time.perf_counter() - start) * 1000, 2)
    return {
        "timestamp": _now_rfc3339(),
        "adapter": adapter_key,
        "status_code": status,
        "latency_ms": elapsed_ms,
        "error": err,
        "body_snippet": str(body)[:200] if body is not None else "",
    }


def run_live_workload(server_url, bearer_token, phases, adapter_mix, output_dir, jitter_ms=100):
    """Run the live workload against the server."""
    headers = _make_headers(bearer_token)
    results = {
        "generated": _now_rfc3339(),
        "mode": "execute",
        "server_url": server_url,
        "disclaimer": "LIVE WORKLOAD EVIDENCE — OPERATOR REVIEW REQUIRED",
        "phases": [],
    }

    all_records = []

    for phase in phases:
        phase_name = phase["name"]
        duration_sec = phase["duration_sec"]
        rate_rps = phase["rate_rps"]
        phase_records = []

        print(f"\n[Phase: {phase_name}] duration={duration_sec}s rate={rate_rps} rps")

        if rate_rps <= 0.0:
            # Idle phase: just wait and probe readyz optionally
            print(f"  Idle phase — sleeping {duration_sec}s")
            time.sleep(duration_sec)
        else:
            interval = 1.0 / rate_rps
            end_time = time.monotonic() + duration_sec
            count = 0
            while time.monotonic() < end_time:
                adapter = _weighted_choice(adapter_mix)
                record = _execute_single_request(server_url, adapter, headers)
                phase_records.append(record)
                all_records.append(record)
                count += 1
                status_str = str(record["status_code"]) if record["status_code"] is not None else "ERR"
                print(f"  req {count}: {adapter} -> HTTP {status_str} in {record['latency_ms']}ms")
                # Sleep with jitter
                sleep_time = interval + random.uniform(-jitter_ms / 1000.0, jitter_ms / 1000.0)
                sleep_time = max(0.01, sleep_time)
                remaining = end_time - time.monotonic()
                if remaining > 0:
                    time.sleep(min(sleep_time, remaining))

        # Phase summary
        latencies = [r["latency_ms"] for r in phase_records if r["status_code"] is not None]
        status_counts = defaultdict(int)
        for r in phase_records:
            status_counts[str(r["status_code"] if r["status_code"] is not None else "ERROR")] += 1

        phase_summary = {
            "name": phase_name,
            "duration_sec": duration_sec,
            "rate_rps": rate_rps,
            "request_count": len(phase_records),
            "status_distribution": dict(status_counts),
            "latency_ms": {
                "p50": _percentile(latencies, 50),
                "p95": _percentile(latencies, 95),
                "p99": _percentile(latencies, 99),
                "min": min(latencies) if latencies else 0.0,
                "max": max(latencies) if latencies else 0.0,
            },
            "errors": [r for r in phase_records if r["error"]],
            "records": phase_records,
        }
        results["phases"].append(phase_summary)

    # Global summary
    all_latencies = [r["latency_ms"] for r in all_records if r["status_code"] is not None]
    all_status_counts = defaultdict(int)
    for r in all_records:
        all_status_counts[str(r["status_code"] if r["status_code"] is not None else "ERROR")] += 1

    results["summary"] = {
        "total_requests": len(all_records),
        "status_distribution": dict(all_status_counts),
        "latency_ms": {
            "p50": _percentile(all_latencies, 50),
            "p95": _percentile(all_latencies, 95),
            "p99": _percentile(all_latencies, 99),
            "min": min(all_latencies) if all_latencies else 0.0,
            "max": max(all_latencies) if all_latencies else 0.0,
        },
    }

    # Write JSON results
    results_file = output_dir / "workload_results.json"
    with open(results_file, "w", encoding="utf-8") as f:
        # Sanitize: do not include full body snippets or tokens in JSON
        safe_results = json.loads(json.dumps(results, default=str))
        # Redact any accidental token leakage in error strings
        _redact_nested(safe_results)
        json.dump(safe_results, f, indent=2)

    # Write Markdown results
    md_file = output_dir / "workload_results.md"
    with open(md_file, "w", encoding="utf-8") as f:
        f.write(DISCLAIMER)
        f.write("\n\n# G3.6 Live Workload Results\n\n")
        f.write(f"*Generated: {_now_rfc3339()}*\n")
        f.write(f"*Server: {server_url}*\n")
        f.write(f"*Mode: EXECUTE (live requests sent)*\n\n")
        f.write("## Global Summary\n\n")
        f.write(f"- **Total requests**: {results['summary']['total_requests']}\n")
        f.write(f"- **Status distribution**: {results['summary']['status_distribution']}\n")
        lat = results["summary"]["latency_ms"]
        f.write(f"- **Latency (ms)**: p50={lat['p50']}, p95={lat['p95']}, p99={lat['p99']}, min={lat['min']}, max={lat['max']}\n")
        f.write("\n## Phase Details\n\n")
        for ps in results["phases"]:
            f.write(f"### {ps['name']}\n\n")
            f.write(f"- Requests: {ps['request_count']}\n")
            f.write(f"- Status distribution: {ps['status_distribution']}\n")
            l = ps["latency_ms"]
            f.write(f"- Latency (ms): p50={l['p50']}, p95={l['p95']}, p99={l['p99']}, min={l['min']}, max={l['max']}\n")
            if ps["errors"]:
                f.write(f"- Errors: {len(ps['errors'])}\n")
                for e in ps["errors"][:5]:
                    f.write(f"  - `{e['error']}`\n")
            f.write("\n")
        f.write("\n---\n*Generated by run_real_workload_generator.py — operator review required.*\n")

    return str(results_file), str(md_file), results


def _redact_nested(obj):
    """Recursively redact bearer tokens from strings in a JSON-serializable object."""
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


# ---------------------------------------------------------------------------
# readyz /deep probe
# ---------------------------------------------------------------------------

def probe_readyz_deep(server_url, bearer_token, output_dir, probe_count=5, probe_interval=10):
    """Probe /v1/readyz/deep repeatedly and record results."""
    headers = _make_headers(bearer_token)
    url = f"{server_url}/v1/readyz/deep"
    results = []
    print(f"\nProbing {url} — {probe_count} probes at {probe_interval}s intervals")

    for i in range(probe_count):
        start = time.perf_counter()
        status, body, err = _make_api_request("GET", url, headers, timeout=30)
        elapsed_ms = round((time.perf_counter() - start) * 1000, 2)
        record = {
            "probe_number": i + 1,
            "timestamp": _now_rfc3339(),
            "url": url,
            "status_code": status,
            "latency_ms": elapsed_ms,
            "body_snippet": str(body)[:500] if body is not None else "",
            "error": err,
        }
        results.append(record)
        status_str = str(status) if status is not None else "TIMEOUT/ERROR"
        print(f"  Probe {i+1}/{probe_count}: HTTP {status_str} in {elapsed_ms}ms")
        if i < probe_count - 1:
            time.sleep(probe_interval)

    json_file = output_dir / "readyz_probe_log.json"
    with open(json_file, "w", encoding="utf-8") as f:
        json.dump(
            {
                "generated": _now_rfc3339(),
                "server_url": server_url,
                "probe_count": probe_count,
                "probe_interval_sec": probe_interval,
                "results": results,
            },
            f,
            indent=2,
        )

    md_file = output_dir / "readyz_probe_log.md"
    with open(md_file, "w", encoding="utf-8") as f:
        f.write(DISCLAIMER)
        f.write(f"\n\n# /v1/readyz/deep Probe Log\n\n")
        f.write(f"*Generated: {_now_rfc3339()}*\n")
        f.write(f"*Server: {server_url}*\n")
        f.write(f"*Probes: {probe_count} at {probe_interval}s intervals*\n\n")
        f.write("| Probe | Timestamp | Status | Latency (ms) | Error |\n")
        f.write("|-------|-----------|--------|--------------|-------|\n")
        for r in results:
            err_cell = f"`{r['error']}`" if r["error"] else "—"
            f.write(f"| {r['probe_number']} | {r['timestamp']} | {r['status_code']} | {r['latency_ms']} | {err_cell} |\n")
        f.write("\n---\n*Generated by run_real_workload_generator.py — operator review required.*\n")

    return str(json_file), str(md_file), results


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="G3.6 Real Workload Generator for FerrumGate",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Safe plan mode (default)
  python3 scripts/run_real_workload_generator.py --plan --server-url https://fg.example.com

  # Live execution (requires bearer token)
  export FERRUM_BEARER_TOKEN="<token>"
  python3 scripts/run_real_workload_generator.py --execute --server-url https://fg.example.com

  # Custom adapter mix
  python3 scripts/run_real_workload_generator.py --plan --server-url https://fg.example.com \\
    --adapter-mix '{"fs":{"weight":50,"intent_type":"FileWrite","tool_name":"fs_write"},"http":{"weight":50,"intent_type":"HttpMutation","tool_name":"http_post"}}'

  # Custom phases
  python3 scripts/run_real_workload_generator.py --plan --server-url https://fg.example.com \\
    --phases '[{"name":"baseline","duration_sec":60,"rate_rps":0},{"name":"spike","duration_sec":60,"rate_rps":10}]'
        """,
    )
    parser.add_argument("--server-url", required=True, help="FerrumGate server base URL")
    parser.add_argument("--bearer-token", default="", help="Bearer token (or set FERRUM_BEARER_TOKEN env var)")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Output directory for evidence files")
    parser.add_argument("--plan", action="store_true", default=True, help="Plan mode: generate plan without live requests (default)")
    parser.add_argument("--execute", action="store_true", default=False, help="Execute mode: send live requests")
    parser.add_argument("--adapter-mix", default="", help="JSON dict of adapter mix (default: 20%% each)")
    parser.add_argument("--phases", default="", help="JSON list of phase definitions")
    parser.add_argument("--readyz-probes", type=int, default=5, help="Number of readyz/deep probes per call")
    parser.add_argument("--readyz-interval", type=int, default=10, help="Interval between readyz probes (seconds)")
    parser.add_argument("--probe-only", action="store_true", help="Only run readyz/deep probe, skip workload")

    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Parse configuration
    adapter_mix = _parse_adapter_mix(args.adapter_mix)
    phases = _parse_phases(args.phases)

    plan_mode = not args.execute

    if args.execute:
        token = args.bearer_token or os.environ.get("FERRUM_BEARER_TOKEN", "")
        if not token:
            print(
                "ERROR: --execute requires a bearer token. "
                "Provide --bearer-token or set FERRUM_BEARER_TOKEN.",
                file=sys.stderr,
            )
            return 1

    if args.probe_only:
        print("\n=== Probe-only mode ===")
        json_path, md_path, _ = probe_readyz_deep(
            args.server_url,
            args.bearer_token,
            output_dir,
            probe_count=args.readyz_probes,
            probe_interval=args.readyz_interval,
        )
        print(f"\nProbe logs written:\n  {json_path}\n  {md_path}")
        return 0

    # Always generate plan first
    plan_json, plan_md, plan = generate_plan(args.server_url, phases, adapter_mix, output_dir)
    print(f"\nPlan written:\n  {plan_json}\n  {plan_md}")

    if plan_mode:
        print("\n=== PLAN mode ===")
        print(f"Estimated total requests: {plan['total_requests_estimated']}")
        print("No live requests were sent. Review the plan, then run with --execute to proceed.")

        # Also run a readyz probe in plan mode for diagnostics (dry-run: no actual probe)
        # Actually, we can do a single lightweight probe to confirm connectivity if desired,
        # but per safety default we skip it. The operator can use --probe-only.
        return 0

    # Execute mode
    print("\n=== EXECUTE mode ===")
    print("WARNING: This will send live requests to the server.")
    print(f"Server: {args.server_url}")
    print(f"Phases: {len(phases)}")
    print(f"Estimated requests: {plan['total_requests_estimated']}")
    print("")

    results_json, results_md, _ = run_live_workload(
        args.server_url,
        args.bearer_token,
        phases,
        adapter_mix,
        output_dir,
    )
    print(f"\nResults written:\n  {results_json}\n  {results_md}")

    # Post-workload readyz probe
    rz_json, rz_md, _ = probe_readyz_deep(
        args.server_url,
        args.bearer_token,
        output_dir,
        probe_count=args.readyz_probes,
        probe_interval=args.readyz_interval,
    )
    print(f"Readyz probe logs written:\n  {rz_json}\n  {rz_md}")

    print("\n=== Done ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
