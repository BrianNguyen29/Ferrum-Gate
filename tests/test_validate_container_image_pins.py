import unittest
from pathlib import Path

import sys

# Import the validator module as a sibling of the scripts package.
REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPTS_DIR = REPO_ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from validate_container_image_pins import (
    MANAGED_DIGESTS,
    MANAGED_FILES,
    check_file,
    expected_ref,
    iter_managed_refs,
    validate_all,
)


class ContainerImagePinningTests(unittest.TestCase):
    """Validate managed container images are pinned to verified digests."""

    def test_managed_files_exist(self):
        missing = [str(p.relative_to(REPO_ROOT)) for p in MANAGED_FILES if not p.exists()]
        self.assertFalse(missing, f"Managed pinning files are missing: {missing}")

    def test_expected_ref_format_is_tag_plus_digest(self):
        for canonical_tag, digest in MANAGED_DIGESTS.items():
            ref = expected_ref(canonical_tag)
            self.assertTrue(
                ref.startswith(f"{canonical_tag}@sha256:"),
                f"Expected ref for {canonical_tag} must be tag@sha256 digest: {ref}",
            )
            self.assertEqual(
                ref,
                f"{canonical_tag}@sha256:{digest}",
                f"Expected ref mismatch for {canonical_tag}",
            )

    def test_all_managed_refs_are_pinned(self):
        errors = validate_all()
        if errors:
            self.fail("Container image pinning validation failed:\n" + "\n".join(errors))

    def test_validator_detects_unpinned_managed_image(self):
        content = "services:\n  db:\n    image: postgres:16\n"
        errors = check_file_with_content(content)
        self.assertTrue(errors)
        self.assertIn("postgres:16", errors[0])

    def test_validator_detects_wrong_digest(self):
        content = "FROM postgres:16@sha256:0000000000000000000000000000000000000000000000000000000000000000\n"
        errors = check_file_with_content(content)
        self.assertTrue(errors)
        self.assertIn("expected", errors[0])

    def test_validator_passes_pinned_managed_image(self):
        content = f"FROM {expected_ref('postgres:16')}\n"
        errors = check_file_with_content(content)
        self.assertFalse(errors)

    def test_iter_managed_refs_is_case_sensitive(self):
        # Uppercase digest should not be accepted by the regex.
        digest = MANAGED_DIGESTS["postgres:16"].upper()
        content = f"image: postgres:16@sha256:{digest}\n"
        refs = iter_managed_refs(content)
        self.assertEqual(len(refs), 1)
        canonical, found = refs[0]
        self.assertEqual(canonical, "postgres:16")
        self.assertIsNone(found)

    def test_registry_prefixed_image_is_not_treated_as_managed(self):
        # A registry-qualified image that merely ends with the managed tag must
        # not be validated against the managed digest.
        content = f"image: myregistry/postgres:16@sha256:{MANAGED_DIGESTS['postgres:16']}\n"
        errors = check_file_with_content(content)
        self.assertFalse(errors)

    def test_unmanaged_image_is_allowed(self):
        # A reference that is not in MANAGED_DIGESTS must not raise an error.
        content = "services:\n  app:\n    image: some-registry/example:1.0\n"
        errors = check_file_with_content(content)
        self.assertFalse(errors)

    def test_postgres_dsn_does_not_match_image_ref(self):
        # postgres:// DSNs are not image references and must not be flagged.
        content = 'DSN="postgres://user:pass@localhost:5432/db"\n'
        errors = check_file_with_content(content)
        self.assertFalse(errors)


def check_file_with_content(content: str) -> list[str]:
    """Helper: run check_file against a temporary file with the given content."""
    import tempfile

    with tempfile.NamedTemporaryFile("w", suffix=".yml", delete=False) as fh:
        fh.write(content)
        path = Path(fh.name)
    try:
        return check_file(path)
    finally:
        path.unlink()


if __name__ == "__main__":
    unittest.main()
