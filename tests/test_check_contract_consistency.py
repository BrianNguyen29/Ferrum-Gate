import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path

scripts_dir = str(Path(__file__).resolve().parents[1] / "scripts")
if scripts_dir not in sys.path:
    sys.path.insert(0, scripts_dir)

import check_contract_consistency as ccc


class SchemaStructureTests(unittest.TestCase):
    """Tests for the new JSON Schema structural validator."""

    def _copy_real_schemas(self, tmp: str) -> tuple[Path, list[Path]]:
        """Copy the real required schemas into a temp directory."""
        schema_dir = Path(tmp) / "schemas"
        schema_dir.mkdir()
        files: list[Path] = []
        for src in ccc.REQUIRED_SCHEMA_FILES:
            data = ccc.load_json(src)
            dest = schema_dir / src.name
            dest.write_text(json.dumps(data), encoding="utf-8")
            files.append(dest)
        return schema_dir, files

    def test_real_schemas_pass(self):
        errors = ccc.check_schema_structure()
        self.assertEqual(errors, [])

    def test_rejects_invalid_draft(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "action-proposal.json"
            data = ccc.load_json(path)
            data["$schema"] = "http://json-schema.org/draft-07/schema#"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("draft/2020-12" in e for e in errors))

    def test_rejects_missing_id(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "action-proposal.json"
            data = ccc.load_json(path)
            del data["$id"]
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("missing $id" in e for e in errors))

    def test_rejects_unstable_relative_id(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "action-proposal.json"
            data = ccc.load_json(path)
            data["$id"] = "action-proposal.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("stable absolute URI" in e for e in errors))

    def test_rejects_duplicate_id(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            for name in ["action-proposal.json", "approval-request.json"]:
                path = schema_dir / name
                data = ccc.load_json(path)
                data["$id"] = "https://ferrumgate.local/schemas/duplicate.json"
                path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("duplicate $id" in e for e in errors))

    def test_rejects_root_not_object(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "action-proposal.json"
            data = ccc.load_json(path)
            data["type"] = "array"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("root type is not 'object'" in e for e in errors))

    def test_rejects_required_without_properties(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "action-proposal.json"
            data = ccc.load_json(path)
            del data["properties"]
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("required present but properties missing" in e for e in errors))

    def test_rejects_required_not_in_properties(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "action-proposal.json"
            data = ccc.load_json(path)
            data["required"].append("nonexistent_field")
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("nonexistent_field" in e and "not in properties" in e for e in errors))

    def test_rejects_unresolved_local_pointer(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "rollback-contract.json"
            data = ccc.load_json(path)
            data["properties"]["verify_checks"]["items"]["$ref"] = "#/properties/nonexistent/items"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("unresolved local JSON Pointer" in e for e in errors))

    def test_rejects_unresolved_sibling_pointer(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "rollback-contract.json"
            data = ccc.load_json(path)
            data["properties"]["rollback_class"]["$ref"] = "common.json#/definitions/Nonexistent"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("unresolved JSON Pointer" in e for e in errors))

    def test_rejects_ref_outside_schema_dir(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "rollback-contract.json"
            data = ccc.load_json(path)
            data["properties"]["rollback_class"]["$ref"] = "../openapi/ferrumgate-control-api.v1.yaml#/components"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("ref outside schema dir" in e for e in errors))

    def test_rejects_external_ref(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "rollback-contract.json"
            data = ccc.load_json(path)
            data["properties"]["rollback_class"]["$ref"] = "https://example.com/schemas/common.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("external ref not allowed" in e for e in errors))

    def test_rejects_empty_enum(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "approval-request.json"
            data = ccc.load_json(path)
            data["properties"]["state"]["enum"] = []
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("enum" in e and "empty" in e for e in errors))

    def test_rejects_duplicate_enum(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "approval-request.json"
            data = ccc.load_json(path)
            data["properties"]["state"]["enum"] = ["Pending", "Granted", "Pending"]
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("duplicate values" in e for e in errors))

    def test_rejects_nonscalar_enum(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "common.json"
            data = ccc.load_json(path)
            data["definitions"]["RiskTier"]["enum"].append({"not": "scalar"})
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("non-scalar" in e for e in errors))

    def test_rejects_enum_contradicting_type(self):
        with tempfile.TemporaryDirectory(dir=ccc.ROOT) as tmp:
            schema_dir, files = self._copy_real_schemas(tmp)
            path = schema_dir / "common.json"
            data = ccc.load_json(path)
            data["definitions"]["RiskTier"]["enum"] = ["Low", 1]
            path.write_text(json.dumps(data), encoding="utf-8")
            errors = ccc.check_schema_structure(files)
        self.assertTrue(any("contradicts type" in e for e in errors))


class ExistingChecksPreservedTests(unittest.TestCase):
    """Verify earlier contract consistency checks remain functional."""

    def test_monitoring_auth_still_passes(self):
        errors = ccc.check_monitoring_auth()
        self.assertEqual(errors, [])

    def test_main_still_passes(self):
        rc = ccc.main()
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
