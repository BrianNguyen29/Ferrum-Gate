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

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `file_restored` | true / false |
| `operator_annotation` | <anomalies or deviations> |

**Operator initials**: _____ **Date**: _________

---

## D2 — Git Local Compensation Drill

### D2.1 GitCommit Compensation Drill

**Command output**:
```
{file_content}
```

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `commit_removed` | true / false |
| `git_status_clean` | true / false |
| `operator_annotation` | <anomalies, dirty worktree, or deviations> |

**Operator initials**: _____ **Date**: _________

---

## D3 — Git Remote Push / Fail-Closed Drill

### D3.1 GitPush Remote Rollback Fail-Closed Verification

**Command output**:
```
{file_content}
```

| Field | Value |
|-------|-------|
| `compensate_http_status` | <200|400|500> |
| `compensation_result` | <describe outcome> |
| `remote_ref_unchanged` | true / false |
| `fail_closed_verified` | true / false |
| `operator_annotation` | Remote permission dependency confirmed; rollback not guaranteed |

**Operator initials**: _____ **Date**: _________

---

## D4 — HTTP Adapter Compensation Drill

### D4.1 HTTP POST Replay Compensation Drill

**Command output**:
```
{file_content}
```

| Field | Value |
|-------|-------|
| `compensate_http_status` | <200|500> |
| `idempotency_replay_verified` | true / false |
| `server_state_changed` | true / false |
| `operator_annotation` | HTTP replay is NOT true undo; confirm idempotency semantics acceptable |

**Operator initials**: _____ **Date**: _________

---

### D4.2 HTTP Non-Idempotent Method Fail-Closed Drill

**Command output**:
```
{file_content}
```

| Field | Value |
|-------|-------|
| `compensate_http_status` | <400|error> |
| `fail_closed_verified` | true / false |
| `operator_annotation` | Non-idempotent without compensation_plan correctly rejected |

**Operator initials**: _____ **Date**: _________

---

## D5 — SQLite Adapter Compensation Drill

**Command output**:
```
{file_content}
```

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `compensation_sql_executed` | true / false |
| `operator_annotation` | <anomalies or deviations> |

**Operator initials**: _____ **Date**: _________

---

## D6 — Maildraft Adapter Compensation Drill

**Command output**:
```
{file_content}
```

| Field | Value |
|-------|-------|
| `recovered` | true / false |
| `draft_removed` | true / false |
| `operator_annotation` | In-memory only; rollback not durable across restart |

**Operator initials**: _____ **Date**: _________

---

## Accepted Exception Fields

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

G2_TEMPLATE = """## G2.1 — Workload Model

**Command output or workload notes**:
```
{file_content}
```

| Metric | Modeled Value | SQLite Phase 1 Limit | Fit? |
|-------|---------------|----------------------|------|
| Expected sustained write rate | _____ writes/s | ≤300 writes/s | YES / NO |
| Expected peak write rate | _____ writes/s | Operator-reviewed | YES / NO |
| Expected daily write volume | _____ writes/day | Operator-reviewed | YES / NO |
| Single-node topology acceptable | YES / NO | Required for Path 2 | YES / NO |

Operator signature: _________________ Date: _________

---

## G2.2 — Auth/TLS Configuration

| Item | Evidence Required | Captured Value |
|------|-------------------|----------------|
| `auth_mode` | Bearer, not disabled | _________________ |
| Bearer token handling | env var or secret manager; redacted in evidence | _________________ |
| TLS termination | reverse proxy config/certificate evidence | _________________ |
| Auth challenge | protected endpoint returns 401 without token | _________________ |

Operator signature: _________________ Date: _________

---

## G2.3 — Backup Schedule

| Item | Evidence Required | Captured Value |
|------|-------------------|----------------|
| Backup tool | `ferrumctl backup create` available | _________________ |
| Scheduler | cron/systemd/other configured | _________________ |
| Retention | retention value and command/log evidence | _________________ |
| Latest backup verify | `ferrumctl backup verify` passes | _________________ |

Operator signature: _________________ Date: _________

---

## G2.4 — Restore Drill

| Field | Value |
|-------|-------|
| `backup_file_used` | /path/to/backup/file.db |
| `backup_verify_pre_restore` | OK / FAILED |
| `restore_completed` | true / false |
| `pre_restore_copy_created` | true / false |
| `backup_verify_post_restore` | OK / FAILED |
| `readyz_deep_returns_200` | true / false |
| `operator_annotation` | <any anomalies or deviations> |

Operator signature: _________________ Date: _________

---

## G2.5 — RPO/RTO Acceptance

| Objective | Modeled Value | Accepted? |
|-----------|---------------|-----------|
| Backup interval / RPO | _____ | YES / NO |
| Restore duration | _____ | YES / NO |
| Restart + verify duration | _____ | YES / NO |
| Total RTO | _____ | YES / NO |

Operator signature: _________________ Date: _________

---

## G2.6 — Production Evaluation

| Dimension | Result | Notes |
|-----------|--------|-------|
| Architecture fit | SATISFIED / CONDITIONAL / BLOCKED | |
| Security posture | SATISFIED / CONDITIONAL / BLOCKED | |
| Operations | SATISFIED / CONDITIONAL / BLOCKED | |
| Recovery | SATISFIED / CONDITIONAL / BLOCKED | |
| Observability | SATISFIED / CONDITIONAL / BLOCKED | |

Operator signature: _________________ Date: _________

---

## G2.7 — Accepted-Risk Review

| Risk / limitation | Accepted? | Compensating control |
|-------------------|-----------|----------------------|
| SQLite single-node only | YES / NO | |
| No PostgreSQL/multi-node/HA in v1 | YES / NO | |
| Compensate may be noop-backed | YES / NO | |
| External backup scheduling required | YES / NO | |
| Health/readiness scope limitations | YES / NO | |

Operator signature: _________________ Date: _________

---

## G2.8 — Compensate Noop Acceptance

**Prerequisite**: D1–D6 drills completed per `58-workload-compensation-drill-evidence-template.md`.

| Adapter | Drill Item | recovered / fail-closed | Accepted Exception? | Operator Initials |
|---------|------------|-------------------------|---------------------|-------------------|
| FS | D1 | true/false | yes/no | |
| Git local | D2 | true/false | yes/no | |
| Git remote | D3 | fail-closed verified | yes/no | |
| HTTP | D4 | true/false or fail-closed | yes/no | |
| SQLite | D5 | true/false | yes/no | |
| Maildraft | D6 | true/false | yes/no | |

Operator signature: _________________ Date: _________

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
