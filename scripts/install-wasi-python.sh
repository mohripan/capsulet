#!/usr/bin/env bash
set -euo pipefail

VERSION="${CAPSULET_WASI_PYTHON_VERSION:-3.13.11}"
WASI_SDK="${CAPSULET_WASI_PYTHON_SDK:-24}"
ROOT="${CAPSULET_WASI_PYTHON_DIR:-.wasi-python}"
DIST="python-${VERSION}-wasi_sdk-${WASI_SDK}"
ZIP="${DIST}.zip"
URL="https://github.com/brettcannon/cpython-wasi-build/releases/download/v${VERSION}/${ZIP}"

mkdir -p "${ROOT}"

if [[ ! -f "${ROOT}/python.wasm" ]]; then
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' EXIT
  echo "Downloading ${URL}" >&2
  curl --fail --location --retry 3 --output "${tmp_dir}/${ZIP}" "${URL}"
  rm -f "${ROOT}/python.wasm"
  rm -rf "${ROOT}/lib"
  unzip -q "${tmp_dir}/${ZIP}" -d "${ROOT}"
fi

if [[ ! -f "${ROOT}/python.wasm" ]]; then
  echo "Expected ${ROOT}/python.wasm after extracting ${ZIP}" >&2
  exit 1
fi

ROOT_ABS="$(cd "${ROOT}" && pwd)"
echo "CAPSULET_WASM_RUNTIME_PATH=${ROOT_ABS}/python.wasm"
