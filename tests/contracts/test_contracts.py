"""Contract checks for the subsystem manifest schema and the gRPC protos.

Invalid manifest fixtures start with a line `# expect-error: <text>`. The fixture must fail
validation, and at least one error must mention <text>, so a fixture can't pass by failing for
an unrelated reason.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest
import yaml
from jsonschema import Draft202012Validator
from jsonschema.exceptions import ValidationError

REPO = Path(__file__).resolve().parents[2]
SCHEMA_PATH = REPO / "specs/001-core-os-foundation/contracts/subsystem-manifest.schema.json"
FIXTURES = REPO / "tests/fixtures/manifests"
BUILTIN_MANIFESTS = REPO / "packaging/subsystems"
PROTO_DIR = REPO / "proto"


@pytest.fixture(scope="session")
def validator() -> Draft202012Validator:
    schema = json.loads(SCHEMA_PATH.read_text())
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


def _load(path: Path) -> object:
    return yaml.safe_load(path.read_text())


def _expected_error(path: Path) -> str:
    first = path.read_text().splitlines()[0]
    prefix = "# expect-error:"
    assert first.startswith(prefix), f"{path.name} must start with '{prefix} <text>'"
    return first[len(prefix) :].strip()


def _describe(error: ValidationError) -> str:
    parts = [error.json_path, error.message, json.dumps(error.schema, default=str)]
    parts += [_describe(child) for child in error.context]
    return " ".join(parts)


def _ids(paths: list[Path]) -> list[str]:
    return [p.name for p in paths]


VALID = sorted((FIXTURES / "valid").glob("*.yaml"))
INVALID = sorted((FIXTURES / "invalid").glob("*.yaml"))
BUILTINS = sorted(BUILTIN_MANIFESTS.glob("*.yaml"))


def test_fixture_sets_are_not_empty() -> None:
    assert VALID, "no valid manifest fixtures"
    assert INVALID, "no invalid manifest fixtures"


@pytest.mark.parametrize("path", VALID, ids=_ids(VALID))
def test_valid_manifest_passes(validator: Draft202012Validator, path: Path) -> None:
    errors = [_describe(e) for e in validator.iter_errors(_load(path))]
    assert not errors, "\n".join(errors)


@pytest.mark.parametrize("path", INVALID, ids=_ids(INVALID))
def test_invalid_manifest_fails_for_the_expected_reason(
    validator: Draft202012Validator, path: Path
) -> None:
    errors = list(validator.iter_errors(_load(path)))
    assert errors, f"{path.name} unexpectedly validated"
    expected = _expected_error(path)
    described = [_describe(e) for e in errors]
    assert any(expected in d for d in described), (
        f"{path.name}: no error mentions {expected!r}:\n" + "\n".join(described)
    )


@pytest.mark.parametrize("path", BUILTINS, ids=_ids(BUILTINS))
def test_builtin_manifest_passes(validator: Draft202012Validator, path: Path) -> None:
    errors = [_describe(e) for e in validator.iter_errors(_load(path))]
    assert not errors, "\n".join(errors)


def _buf() -> str | None:
    return os.environ.get("BUF") or shutil.which("buf")


def _run_buf(*args: str) -> subprocess.CompletedProcess[str]:
    buf = _buf()
    if buf is None:
        pytest.skip("buf is not installed (set BUF to its path)")
    return subprocess.run([buf, *args], cwd=REPO, capture_output=True, text=True, check=False)


def test_proto_lint() -> None:
    result = _run_buf("lint", "proto")
    assert result.returncode == 0, result.stdout + result.stderr


def test_proto_breaking_against_base() -> None:
    base = os.environ.get("BUF_BREAKING_BASE", "main")
    has_base_proto = subprocess.run(
        ["git", "cat-file", "-e", f"{base}:proto/buf.yaml"], cwd=REPO, capture_output=True
    )
    if has_base_proto.returncode != 0:
        pytest.skip(f"{base} has no proto/ module yet, so there is nothing to compare against")
    result = _run_buf("breaking", "proto", "--against", f".git#ref={base},subdir=proto")
    assert result.returncode == 0, result.stdout + result.stderr
