#!/usr/bin/env bash
set -e
export PATH="/tmp/bin:$PATH"
echo "Generating Python Protobuf code..."
python3 -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. proto/*.proto || true
echo "Protobuf generation script finished."
