#!/usr/bin/env bash
# Generate Python protobuf modules into gen/python (gitignored).
# The mesh itself exchanges JSON; generated types are for consumers.
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-gen/python}"
mkdir -p "$out"
python3 -m grpc_tools.protoc -I. --python_out="$out" proto/*.proto
echo "Generated Python protobuf modules in $out"
