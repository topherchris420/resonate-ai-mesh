import asyncio
import os
import json
import websockets
from biometric_generator import BiometricPipeline

async fn_main():
    ws_url = os.getenv("TELEMETRY_BRIDGE_WS", "ws://127.0.0.1:8080")
    mode = os.getenv("MODE", "SIMULATED")
    rate = float(os.getenv("SAMPLE_RATE_HZ", "10.0"))

    pipeline = BiometricPipeline(operator_id="operator_alpha", mode=mode, sample_rate_hz=rate)
    print(f"Starting Biometric Pipeline service [Mode={pipeline.mode}, Rate={rate}Hz] target: {ws_url}")

    try:
        async with websockets.connect(ws_url) as websocket:
            while True:
                sample = pipeline.generate_sample()
                envelope = {
                    "event_id": f"bio_{sample['timestamp']}",
                    "event_type": "telemetry",
                    "schema_version": "1.0.0",
                    "timestamp": sample["timestamp"],
                    "source": "biometric_pipeline",
                    "subject_id": sample["operator_id"],
                    "correlation_id": f"corr_bio_{sample['timestamp']}",
                    "causation_id": f"caus_bio_{sample['timestamp']}",
                    "provenance": sample["sensor_provenance"],
                    "payload_json": json.dumps(sample)
                }
                await websocket.send(json.dumps(envelope))
                await asyncio.sleep(1.0 / rate)
    except Exception as e:
        print(f"Biometric Pipeline WebSocket connection ended/failed: {e}")

if __name__ == "__main__":
    asyncio.run(fn_main())
