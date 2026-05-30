#!/usr/bin/env python3
"""Create a deterministic local SQLite fixture for PG migration/restore drills."""

from __future__ import annotations

import argparse
import json
import sqlite3
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
SQLITE_MIGRATIONS = [
    REPO_ROOT / "crates" / "ferrum-store" / "migrations" / "001_initial.sql",
    REPO_ROOT / "crates" / "ferrum-store" / "migrations" / "005_add_policy_bundles.sql",
]

CORE_TABLES = [
    "intents",
    "proposals",
    "capabilities",
    "executions",
    "rollback_contracts",
    "approvals",
    "provenance_events",
    "provenance_edges",
    "ledger_entries",
    "policy_bundles",
]


def canonical_json(payload: dict) -> str:
    return json.dumps(payload, separators=(",", ":"), sort_keys=True)


def apply_migrations(conn: sqlite3.Connection) -> None:
    for path in SQLITE_MIGRATIONS:
        conn.executescript(path.read_text(encoding="utf-8"))


def seed_fixture(conn: sqlite3.Connection) -> None:
    ts0 = "2026-05-25T00:00:00Z"
    ts1 = "2026-05-25T00:00:01Z"

    intent_id = "intent-local-pg-1"
    proposal_id = "proposal-local-pg-1"
    capability_id = "capability-local-pg-1"
    execution_id = "execution-local-pg-1"
    rollback_contract_id = "rollback-local-pg-1"
    approval_id = "approval-local-pg-1"
    bundle_id = "policy-local-pg-1"
    event_1 = "event-local-pg-1"
    event_2 = "event-local-pg-2"

    conn.execute("PRAGMA foreign_keys = ON")

    intent = {
        "intent_id": intent_id,
        "principal_id": "principal-local-pg-1",
        "normalized_goal": "validate local postgresql migration",
        "status": "PendingApproval",
        "risk_tier": "Low",
        "approval_mode": "None",
        "default_rollback_class": "R1SnapshotRecoverable",
        "created_at": ts0,
        "expires_at": ts1,
    }
    conn.execute(
        """
        INSERT INTO intents (
            intent_id, principal_id, normalized_goal, status, risk_tier,
            approval_mode, default_rollback_class, created_at, expires_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            intent_id,
            intent["principal_id"],
            intent["normalized_goal"],
            intent["status"],
            intent["risk_tier"],
            intent["approval_mode"],
            intent["default_rollback_class"],
            intent["created_at"],
            intent["expires_at"],
            canonical_json(intent),
        ),
    )

    proposal = {
        "proposal_id": proposal_id,
        "intent_id": intent_id,
        "step_index": 0,
        "server_name": "local-drill",
        "tool_name": "pg-local-batch",
        "estimated_risk": "Low",
        "requested_rollback_class": "R1SnapshotRecoverable",
        "created_at": ts0,
    }
    conn.execute(
        """
        INSERT INTO proposals (
            proposal_id, intent_id, step_index, server_name, tool_name,
            estimated_risk, requested_rollback_class, created_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            proposal_id,
            intent_id,
            proposal["step_index"],
            proposal["server_name"],
            proposal["tool_name"],
            proposal["estimated_risk"],
            proposal["requested_rollback_class"],
            proposal["created_at"],
            canonical_json(proposal),
        ),
    )

    capability = {
        "capability_id": capability_id,
        "intent_id": intent_id,
        "proposal_id": proposal_id,
        "server_name": "local-drill",
        "tool_name": "pg-local-batch",
        "status": "Active",
        "issued_at": ts0,
        "expires_at": ts1,
        "revoked_at": None,
    }
    conn.execute(
        """
        INSERT INTO capabilities (
            capability_id, intent_id, proposal_id, server_name, tool_name,
            status, issued_at, expires_at, revoked_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            capability_id,
            intent_id,
            proposal_id,
            capability["server_name"],
            capability["tool_name"],
            capability["status"],
            capability["issued_at"],
            capability["expires_at"],
            capability["revoked_at"],
            canonical_json(capability),
        ),
    )

    execution = {
        "execution_id": execution_id,
        "intent_id": intent_id,
        "proposal_id": proposal_id,
        "capability_id": capability_id,
        "rollback_contract_id": None,
        "decision": "Allow",
        "state": "Succeeded",
        "started_at": ts0,
        "finished_at": ts1,
        "result_digest": "result-local-pg-1",
    }
    conn.execute(
        """
        INSERT INTO executions (
            execution_id, intent_id, proposal_id, capability_id, rollback_contract_id,
            decision, state, started_at, finished_at, result_digest, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            execution_id,
            intent_id,
            proposal_id,
            capability_id,
            execution["rollback_contract_id"],
            execution["decision"],
            execution["state"],
            execution["started_at"],
            execution["finished_at"],
            execution["result_digest"],
            canonical_json(execution),
        ),
    )

    rollback_contract = {
        "contract_id": rollback_contract_id,
        "intent_id": intent_id,
        "proposal_id": proposal_id,
        "execution_id": execution_id,
        "adapter_key": "fs",
        "action_type": "FileWrite",
        "rollback_class": "R1SnapshotRecoverable",
        "state": "Prepared",
        "auto_commit": 0,
        "created_at": ts0,
        "expires_at": ts1,
    }
    conn.execute(
        """
        INSERT INTO rollback_contracts (
            contract_id, intent_id, proposal_id, execution_id, adapter_key, action_type,
            rollback_class, state, auto_commit, created_at, expires_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            rollback_contract_id,
            intent_id,
            proposal_id,
            execution_id,
            rollback_contract["adapter_key"],
            rollback_contract["action_type"],
            rollback_contract["rollback_class"],
            rollback_contract["state"],
            rollback_contract["auto_commit"],
            rollback_contract["created_at"],
            rollback_contract["expires_at"],
            canonical_json(rollback_contract),
        ),
    )

    approval = {
        "approval_id": approval_id,
        "intent_id": intent_id,
        "proposal_id": proposal_id,
        "execution_id": execution_id,
        "action_digest": "digest-local-pg-1",
        "state": "Pending",
        "expires_at": ts1,
        "created_at": ts0,
    }
    conn.execute(
        """
        INSERT INTO approvals (
            approval_id, intent_id, proposal_id, execution_id, action_digest,
            state, expires_at, created_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            approval_id,
            intent_id,
            proposal_id,
            execution_id,
            approval["action_digest"],
            approval["state"],
            approval["expires_at"],
            approval["created_at"],
            canonical_json(approval),
        ),
    )

    policy_bundle = {
        "bundle_id": bundle_id,
        "version": "v1",
        "active": 1,
        "content_hash": "policy-hash-local-pg-1",
        "created_at": ts0,
        "updated_at": ts1,
    }
    conn.execute(
        """
        INSERT INTO policy_bundles (
            bundle_id, version, active, content_hash, created_at, updated_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
        (
            bundle_id,
            policy_bundle["version"],
            policy_bundle["active"],
            policy_bundle["content_hash"],
            policy_bundle["created_at"],
            policy_bundle["updated_at"],
            canonical_json(policy_bundle),
        ),
    )

    provenance_event_1 = {
        "event_id": event_1,
        "kind": "IntentCreated",
        "occurred_at": ts0,
        "intent_id": intent_id,
        "proposal_id": proposal_id,
        "execution_id": execution_id,
        "capability_id": capability_id,
        "rollback_contract_id": rollback_contract_id,
        "policy_bundle_id": bundle_id,
    }
    provenance_event_2 = {
        "event_id": event_2,
        "kind": "ExecutionFinished",
        "occurred_at": ts1,
        "intent_id": intent_id,
        "proposal_id": proposal_id,
        "execution_id": execution_id,
        "capability_id": capability_id,
        "rollback_contract_id": rollback_contract_id,
        "policy_bundle_id": bundle_id,
    }
    for event in (provenance_event_1, provenance_event_2):
        conn.execute(
            """
            INSERT INTO provenance_events (
                event_id, kind, occurred_at, intent_id, proposal_id, execution_id,
                capability_id, rollback_contract_id, policy_bundle_id, raw_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                event["event_id"],
                event["kind"],
                event["occurred_at"],
                event["intent_id"],
                event["proposal_id"],
                event["execution_id"],
                event["capability_id"],
                event["rollback_contract_id"],
                event["policy_bundle_id"],
                canonical_json(event),
            ),
        )

    conn.execute(
        """
        INSERT INTO provenance_edges (to_event_id, from_event_id, edge_type, summary)
        VALUES (?, ?, ?, ?)
        """,
        (event_2, event_1, "DerivedFrom", "local pg drill lineage"),
    )

    ledger_entry_1 = {
        "entry_id": 1,
        "event_id": event_1,
        "intent_id": intent_id,
        "execution_id": execution_id,
        "occurred_at": ts0,
        "content_hash": "ledger-hash-local-pg-1",
        "previous_ledger_hash": None,
    }
    ledger_entry_2 = {
        "entry_id": 2,
        "event_id": event_2,
        "intent_id": intent_id,
        "execution_id": execution_id,
        "occurred_at": ts1,
        "content_hash": "ledger-hash-local-pg-2",
        "previous_ledger_hash": "ledger-hash-local-pg-1",
    }
    for entry in (ledger_entry_1, ledger_entry_2):
        conn.execute(
            """
            INSERT INTO ledger_entries (
                entry_id, event_id, intent_id, execution_id, occurred_at,
                content_hash, previous_ledger_hash, raw_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                entry["entry_id"],
                entry["event_id"],
                entry["intent_id"],
                entry["execution_id"],
                entry["occurred_at"],
                entry["content_hash"],
                entry["previous_ledger_hash"],
                canonical_json(entry),
            ),
        )

    conn.commit()


def collect_counts(conn: sqlite3.Connection) -> dict[str, int]:
    return {
        table: conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        for table in CORE_TABLES
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db-path", required=True, help="Output SQLite database path")
    args = parser.parse_args()

    db_path = Path(args.db_path).resolve()
    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()

    conn = sqlite3.connect(db_path)
    try:
        apply_migrations(conn)
        seed_fixture(conn)
        summary = {
            "db_path": str(db_path),
            "counts": collect_counts(conn),
        }
    finally:
        conn.close()

    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
