"""Stream simulated human-state data to the mesh ingest endpoint.

    MESH_INGEST_URL=ws://127.0.0.1:7878/ingest python main.py

The mesh admits the data only if it passes its schema and labeling checks;
every reply is printed. Start a live session with `"human_state": "ingest"`
to feed these datums into the kernel instead of the built-in simulation.
"""

import asyncio
import json
import os
import sys
import urllib.parse

import websockets

from biometric_generator import BiometricPipeline, LiveSourceUnavailable, human_state_envelope


def ingest_url() -> str:
    url = os.getenv("MESH_INGEST_URL", "ws://127.0.0.1:7878/ingest")
    token = os.getenv("MESH_API_TOKEN")
    if token:
        separator = "&" if "?" in url else "?"
        url = f"{url}{separator}token={urllib.parse.quote(token)}"
    return url


async def stream(pipeline: BiometricPipeline, rate_hz: float, limit: int) -> int:
    rejected = 0
    async with websockets.connect(ingest_url(), max_size=2**16) as socket:
        sent = 0
        while limit <= 0 or sent < limit:
            sample = pipeline.generate_sample()
            for datum in pipeline.to_datums(sample):
                await socket.send(json.dumps(human_state_envelope(datum, pipeline.operator_id, sent)))
                reply = json.loads(await socket.recv())
                if not reply.get("accepted"):
                    rejected += 1
                    print(f"rejected: {reply.get('code')}: {reply.get('error')}", file=sys.stderr)
            sent += 1
            await asyncio.sleep(1.0 / rate_hz)
    return rejected


def main() -> int:
    mode = os.getenv("MODE", "SIMULATED")
    rate = float(os.getenv("SAMPLE_RATE_HZ", "4"))
    limit = int(os.getenv("SAMPLE_LIMIT", "0"))
    try:
        pipeline = BiometricPipeline(mode=mode, sample_rate_hz=rate, seed=int(os.getenv("SEED", "42")))
    except LiveSourceUnavailable as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    print(f"streaming {pipeline.mode} human-state data at {rate} Hz to {os.getenv('MESH_INGEST_URL', 'ws://127.0.0.1:7878/ingest')}")
    try:
        rejected = asyncio.run(stream(pipeline, rate, limit))
    except (OSError, websockets.exceptions.WebSocketException) as error:
        print(f"error: cannot stream to the mesh: {error}", file=sys.stderr)
        return 1
    return 1 if rejected else 0


if __name__ == "__main__":
    sys.exit(main())
