#!/usr/bin/env python3
"""
Generate operator-fillable evidence skeleton markdown from command output files or stdin.

This script produces skeleton evidence sections for FerrumGate v1 compensation drills
(D1-D6) and pilot readiness items (G2.1-G2.8). It does NOT complete G2 or claim
operator signoff — those remain operator-owned actions.

Usage:
    # From stdin:
    $ cat drill_output.txt | python3 scripts/generate_evidence_skeleton.py --type d1-d6

    # From file(s):
    $ python3 scripts/generate_evidence_skeleton.py --type g2 --file g2_output.txt

    # Multiple files:
    $ python3 scripts/generate_evidence_skeleton.py --type d1-d6 --file d1.txt d2.txt

    # Help:
    $ python3 scripts/generate_evidence_skeleton.py --help

Types:
    d1-d6   Compensation drill evidence (D1.1 through D6)
    g2      Pilot readiness evidence (G2.1 through G2.8)
    all     Both d1-d6 and g2

Output is markdown suitable for copying into:
    - docs/implementation-path/58-workload-compensation-drill-evidence-template.md
    - docs/implementation-path/59-pilot-readiness-evidence-packet.md
"""

import argparse
import sys
from datetime import date

REPO_ROOT = "."

DISCLAIMER = """
> **⚠️  OPERATOR REVIEW REQUIRED — DO NOT MARK COMPLETE**
>
> This skeleton was auto-generated from supplied command output and is NOT valid
> evidence until reviewed, annotated, and signed by the operator. G2 items remain
> pending/operator-owned. See:
>   - D1-D6 drills: `docs/implementation-path/58-workload-compensation-drill-evidence-template.md`
>   - G2 pilot readiness: `docs/implementation-path/59-pilot-readiness-evidence-packet.md`
>
> **No production pilot signoff is implied or granted by this output.**

---

"""

D1_D6_TEMPLATE = """## D1 — FS Adapter Compensation Drill

### D1.1 FileWrite Compensation Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `file_state_after_compensate` | <observed content or absence> |
| `compensate_http_status` | <200|400|500> |
| `operator_annotation` | <any anomalies, timing, or deviations observed> |

**Operator initials**: _____ **Date**: _________

---

### D1.2 FileDelete Compensation Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `file_restored` | true / false |
| `operator_annotation` | <anomalies or deviations> |

**Operator initials**: _____ **Date**: _________

---

## D2 — Git Adapter Compensation Drill

### D2.1 GitCommit Compensation Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `commit_removed` | true / false |
| `git_status_clean` | true / false |
| `operator_annotation` | <anomalies, dirty worktree, or deviations> |

**Operator initials**: _____ **Date**: _________

---

### D2.2 GitPush Compensation Drill (Fail-Closed Verification)

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `compensate_http_status` | <200> |
| `compensation_result` | <describe outcome> |
| `remote_ref_unchanged` | true / false |
| `fail_closed_verified` | true / false |
| `operator_annotation` | Remote permission dependency confirmed; rollback not guaranteed |

**Operator initials**: _____ **Date**: _________

---

## D3 — HTTP Adapter Compensation Drill

### D3.1 HTTP POST Replay Compensation Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `compensate_http_status` | <200|500> |
| `idempotency_replay_verified` | true / false |
| `server_state_changed` | true / false |
| `operator_annotation` | HTTP replay is NOT true undo; confirm idempotency semantics acceptable |

**Operator initials**: _____ **Date**: _________

---

### D3.2 HTTP Non-Idempotent Method Fail-Closed Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `compensate_http_status` | <400|error> |
| `fail_closed_verified` | true / false |
| `operator_annotation` | Non-idempotent without compensation_plan correctly rejected |

**Operator initials**: _____ **Date**: _________

---

## D4 — SQLite Adapter Compensation Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `compensation_sql_executed` | true / false |
| `operator_annotation` | <anomalies or deviations> |

**Operator initials**: _____ **Date**: _________

---

## D5 — Maildraft Adapter Compensation Drill

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `draft_removed` | true / false |
| `operator_annotation` | In-memory only; rollback not durable across restart |

**Operator initials**: _____ **Date**: _________

---

## D6 — Accepted Exception Fields

For any drill item where `recovered: false` or unexpected behavior observed:

| Field | Description |
|-------|-------------|
| `exception_item` | D1–D6 item identifier |
| `failure_mode` | What happened instead of expected recovery |
| `root_cause_hypothesis` | Operator's assessment of why recovery failed |
| `acceptable_risk` | true/false — whether this exception is acceptable for the target workload |
| `compensating_control` | Any operator-implemented control to mitigate this exception |
| `operator_signoff` | Operator initials and date confirming acceptance or rejection |

---

"""

G2_TEMPLATE = """## G2.1 — Backup/Restore Drill Evidence

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Field | Value |
|-------|-------|
| `backup_file_used` | /path/to/backup/file.db |
| `backup_verify_pre_restore` | OK / FAILED |
| `restore_completed` | true / false |
| `pre_restore_copy_created` | true / false |
| `backup_verify_post_restore` | OK / FAILED |
| `ferrumd_restarted` | true / false |
| `readyz_deep_returns_200` | true / false |
| `operator_annotation` | <any anomalies or deviations> |

**Signoff phrase**: "Operator has performed a restore drill in a non-production environment,
confirmed `PRAGMA integrity_check` passes on the restored database, and verified server
restart succeeds."

Operator signature: _________________ Date: _________

---

## G2.2 — Compensation Drill Evidence

**Prerequisite**: D1–D6 drills completed per `58-workload-compensation-drill-evidence-template.md`.

**Reference to Drill Results**:

| Adapter | Drill Item | recovered | Accepted Exception? | Operator Initials |
|---------|-----------|-----------|---------------------|------------------|
| FS (FileWrite) | D1.1 | true/false | yes/no | |
| FS (FileDelete) | D1.2 | true/false | yes/no | |
| Git (GitCommit) | D2.1 | true/false | yes/no | |
| Git (GitPush) | D2.2 | N/A (fail-closed) | yes/no | |
| HTTP (POST replay) | D3.1 | true/false | yes/no | |
| HTTP (fail-closed) | D3.2 | verified | yes/no | |
| SQLite (DML) | D4 | true/false | yes/no | |
| Maildraft | D5 | true/false | yes/no | |

**Signoff phrase**: "Operator has executed compensation drills for target adapters and
accepts the compensate noop risk as documented. For adapters where `recovered: false`,
compensating controls have been accepted or the risk has been formally accepted."

Operator signature: _________________ Date: _________

---

## G2.3 — Readiness Probe Evidence

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Probe | HTTP Status | Pass? |
|-------|-------------|-------|
| `healthz` | 200 / other | |
| `readyz` | 200 / other | |
| `readyz/deep` | 200 / 503 / other | |
| `approvals?limit=1` (authed) | 200 / other | |

**Signoff phrase**: "Operator has confirmed `readyz/deep` returns HTTP 200 and functional
probe succeeds. Health endpoints are intentionally shallow; functional probe is used for
governance loop confirmation."

Operator signature: _________________ Date: _________

---

## G2.4 — Metrics Baseline Evidence

**Command output**:
```
{file_content}
```

**Evidence fields**:

| Metric | Expected | Observed |
|--------|----------|----------|
| `ferrumgate_http_requests_total` present | yes | |
| `ferrumgate_store_health_up` equals 1 | yes | |
| `ferrumgate_metrics_scrapes_total` increments | yes | |
| Prometheus format valid | yes | |

**Signoff phrase**: "Operator has confirmed `/v1/metrics` returns Prometheus-compatible
output with expected metrics. Scope limitations (no latency histograms, no per-route error
counters, no WAL metrics) are accepted."

Operator signature: _________________ Date: _________

---

## G2.5 — Known Limitation Acceptance

**Known Limitations Checklist**:

| Limitation | Reference | Accepted (Y/N) | Operator Initials |
|------------|-----------|----------------|------------------|
| Healthz/readyz are shallow probes | `21-v1-single-node-observability-minimums.md` §3.2 | | |
| `/v1/metrics` has limited metrics | `21-v1-single-node-observability-minimums.md` §3.3 | | |
| Compensate may be noop-backed | `27-production-evaluation-plan.md` §3.6 | | |
| GitPush rollback is fail-closed | `54-adapter-compensation-matrix.md` §Git | | |
| HTTP replay is NOT true undo | `54-adapter-compensation-matrix.md` §HTTP | | |
| Maildraft in-memory only | `54-adapter-compensation-matrix.md` §Maildraft | | |
| No incremental backup | `27-production-evaluation-plan.md` §3.5 | | |
| No built-in backup scheduling | `18-single-node-operations-runbook.md` §5.4 | | |
| RPO = time since last backup | `27-production-evaluation-plan.md` §3.5 | | |
| RTO includes manual restore + restart | `18-single-node-operations-runbook.md` §7 | | |
| SQLite single-node only (no HA/replica) | `19-v1-single-node-support-contract.md` | | |
| Phase 2 transaction batching deferred | `31-release-paths-todo.md` | | |
| PostgreSQL not implemented | `27-production-evaluation-plan.md` §4 | | |

**Signoff phrase**: "Operator has reviewed all known limitations above and accepts them
as documented constraints of the v1 single-node SQLite deployment."

Operator signature: _________________ Date: _________

---

## G2.6 — Rollback Acceptance

**Command output** (if applicable):
```
{file_content}
```

**Evidence fields**:

| Item | Verification |
|------|-------------|
| `rollback_class` correctly set at intent creation | Operator confirms caller sets correct class |
| R3 `auto_commit=false` verified at prepare | Confirmed per `26-v1-single-node-invariant-control-test-evidence-matrix.md` |
| Rollback/compensate distinction understood | Operator confirms understanding |

**Signoff phrase**: "Operator understands rollback class semantics and confirms caller
responsibility for correct `rollback_class` at intent creation. R3 `auto_commit=false`
control verified by integration test evidence."

Operator signature: _________________ Date: _________

---

## G2.7 — SQLite Capacity Acceptance

**Workload Model Evidence**:

| Parameter | Modeled Value | Capacity Limit | Fit (Y/N) |
|-----------|---------------|----------------|-----------|
| Expected sustained write rate (writes/s) | | ≤300 | |
| Expected peak write rate (writes/s) | | ≤500 (brief) | |
| Single-node topology confirmed | | Yes (no multi-node) | |
| Bounded execution history acceptable | | Yes/No | |

**Source of workload model**: <operator-specified source document>

**Signoff phrase**: "Operator has modeled the expected production workload against
SQLite single-node capacity limits and confirms fit for the target pilot workload."

Operator signature: _________________ Date: _________

---

## G2.8 — Operator Signoff

**Gate criterion**: All G2.1–G2.7 satisfied; formal operator acceptance statement signed.

### G2.8 — Final Acceptance Statement

> **Operator acceptance**: "I, [Operator Name], acting in my capacity as [Role], have
> evaluated FerrumGate v1 single-node SQLite against the production evaluation plan
> (`27-production-evaluation-plan.md`). I have reviewed and accepted all known limitations,
> completed all G2.1–G2.7 evidence items above, executed the compensation drills per
> `58-workload-compensation-drill-evidence-template.md`, and formally accept the compensate
> noop risk for the target adapters in the production pilot. I confirm the workload fits
> within Phase 1 SQLite constraints and I authorize the limited production pilot deployment
> as described in `31-release-paths-todo.md` §Path 2. **This is not a production-ready claim.**
> FerrumGate v1 remains RC-ready/conditional."

### G2.8 — Signature Block

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Operator | | | |
| Owner/Supervisor countersignature (if required) | | | |

---

"""


def read_input_files(file_paths):
    """Read and concatenate content from input files."""
    contents = []
    for path in file_paths:
        try:
            with open(path, "r", encoding="utf-8") as f:
                contents.append(f.read())
        except FileNotFoundError:
            print(f"Warning: File not found: {path}", file=sys.stderr)
        except Exception as e:
            print(f"Warning: Error reading {path}: {e}", file=sys.stderr)
    return "\n---\n".join(contents) if contents else "<command output not provided>"


def read_stdin():
    """Read all content from stdin."""
    if sys.stdin.isatty():
        return None
    return sys.stdin.read()


def generate_d1_d6(content):
    """Generate D1-D6 compensation drill evidence skeleton."""
    output = DISCLAIMER
    output += "# D1–D6 Compensation Drill Evidence Skeleton\n\n"
    output += f"*Generated: {date.today()} — Operator review required before use*\n\n"
    output += DISCLAIMER
    output += D1_D6_TEMPLATE.replace("{file_content}", content)
    return output


def generate_g2(content):
    """Generate G2.1-G2.8 pilot readiness evidence skeleton."""
    output = DISCLAIMER
    output += "# G2 Pilot Readiness Evidence Skeleton\n\n"
    output += f"*Generated: {date.today()} — Operator review required before use*\n\n"
    output += DISCLAIMER
    output += G2_TEMPLATE.replace("{file_content}", content)
    return output


def generate_all(content):
    """Generate both D1-D6 and G2 evidence skeletons."""
    output = DISCLAIMER
    output += "# D1–D6 + G2 Evidence Skeleton\n\n"
    output += f"*Generated: {date.today()} — Operator review required before use*\n\n"
    output += DISCLAIMER
    output += D1_D6_TEMPLATE.replace("{file_content}", content)
    output += "\n\n---\n\n"
    output += G2_TEMPLATE.replace("{file_content}", content)
    return output


def main():
    parser = argparse.ArgumentParser(
        description="Generate operator-fillable evidence skeleton markdown from command output.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # From stdin:
  cat drill_output.txt | python3 scripts/generate_evidence_skeleton.py --type d1-d6

  # From file(s):
  python3 scripts/generate_evidence_skeleton.py --type g2 --file g2_output.txt

  # Multiple files:
  python3 scripts/generate_evidence_skeleton.py --type d1-d6 --file d1.txt d2.txt

  # All evidence types:
  python3 scripts/generate_evidence_skeleton.py --type all
        """,
    )
    parser.add_argument(
        "--type",
        choices=["d1-d6", "g2", "all"],
        default="all",
        help="Type of evidence skeleton to generate (default: all)",
    )
    parser.add_argument(
        "--file",
        nargs="*",
        metavar="PATH",
        help="Input file(s) containing command output. If not provided, reads from stdin.",
    )

    args = parser.parse_args()

    # Get content from files or stdin
    if args.file:
        content = read_input_files(args.file)
    else:
        stdin_content = read_stdin()
        if stdin_content is None:
            print("Error: No input provided. Use --file or pipe content via stdin.", file=sys.stderr)
            print("Run with --help for usage information.", file=sys.stderr)
            sys.exit(1)
        content = stdin_content

    # Generate skeleton
    if args.type == "d1-d6":
        output = generate_d1_d6(content)
    elif args.type == "g2":
        output = generate_g2(content)
    else:
        output = generate_all(content)

    print(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
