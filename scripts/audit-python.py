"""Audit declared SDK runtime, optional and build dependencies in a disposable environment."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib

AUDITOR_VERSION = "2.10.1"
ROOT = Path(__file__).resolve().parents[1]


def requirements(manifest):
    document = tomllib.loads(manifest.read_text(encoding="utf-8"))
    project = document["project"]
    if "dependencies" in project.get("dynamic", []):
        raise ValueError("dynamic SDK dependencies require a reviewed audit adapter")
    groups = [project.get("dependencies", []), document["build-system"]["requires"]]
    groups.extend(project.get("optional-dependencies", {}).values())
    values = []
    for group in groups:
        if not isinstance(group, list) or any(not isinstance(value, str) or not value.strip() for value in group):
            raise ValueError("invalid Python dependency declaration")
        values.extend(group)
    return sorted(set(values))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=ROOT / "sdk/python/pyproject.toml")
    parser.add_argument("--list-requirements", action="store_true", help="inspect declarations only; NOT an audit")
    arguments = parser.parse_args()
    declared = requirements(arguments.manifest)
    if arguments.list_requirements:
        print(json.dumps(declared))
        return
    artifact = ROOT / ".capsulet/security/python-audit.json"
    artifact.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="capsulet-python-audit-") as temporary:
        environment = Path(temporary) / "venv"
        subprocess.run([sys.executable, "-m", "venv", str(environment)], check=True)
        executable = environment / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
        subprocess.run([str(executable), "-m", "pip", "install", "--disable-pip-version-check",
                        f"pip-audit=={AUDITOR_VERSION}"], check=True)
        dependency_file = Path(temporary) / "requirements.txt"
        dependency_file.write_text("\n".join(declared) + "\n", encoding="utf-8")
        result = subprocess.run([str(executable), "-m", "pip_audit", "--strict", "--progress-spinner", "off",
                                 "--format", "json", "--requirement", str(dependency_file)],
                                capture_output=True, text=True, check=False)
        print(result.stderr, file=sys.stderr, end="")
        if result.returncode != 0:
            artifact.write_text(result.stdout, encoding="utf-8")
            raise RuntimeError(f"Python dependency audit failed ({result.returncode}); see {artifact}")
        report = json.loads(result.stdout)
        if not isinstance(report.get("dependencies"), list):
            raise ValueError("invalid Python audit report: dependency results missing")
        if any(item.get("vulns") for item in report["dependencies"]):
            raise ValueError("Python auditor returned success with vulnerability findings")
        report["capsulet_auditor_version"] = AUDITOR_VERSION
        report["capsulet_declared_requirements"] = declared
        artifact.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"Python audit passed: {len(report['dependencies'])} resolved runtime/optional/build packages; pip-audit {AUDITOR_VERSION}")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, ValueError, RuntimeError, OSError, subprocess.CalledProcessError) as error:
        print(f"Python audit error: {error}", file=sys.stderr)
        sys.exit(1)
