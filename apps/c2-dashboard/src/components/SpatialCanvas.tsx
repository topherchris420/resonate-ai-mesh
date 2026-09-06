 "use client";

import React, { useRef } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { OrbitControls, Text } from "@react-three/drei";
import * as THREE from "three";
import { AgentState } from "@pordenone/shared-types";

interface SpatialCanvasProps {
  agents: AgentState[];
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  visualDensityScale: number;
}

function TerrainMesh() {
  const meshRef = useRef<THREE.Mesh>(null);

  const geometry = React.useMemo(() => {
    const geo = new THREE.PlaneGeometry(1200, 1200, 40, 40);
    geo.rotateX(-Math.PI / 2);
    const pos = geo.attributes.position;
    for (let i = 0; i < pos.count; i++) {
      const x = pos.getX(i);
      const z = pos.getZ(i);
      const h = Math.sin(x * 0.01) * Math.cos(z * 0.01) * 20.0 + Math.sin(x * 0.03) * 10.0;
      pos.setY(i, h);
    }
    geo.computeVertexNormals();
    return geo;
  }, []);

  return (
    <mesh ref={meshRef} geometry={geometry}>
      <meshStandardMaterial color="#1a2332" wireframe opacity={0.6} transparent />
    </mesh>
  );
}

function AgentMarker({
  agent,
  isSelected,
  onSelect,
}: {
  agent: AgentState;
  isSelected: boolean;
  onSelect: () => void;
}) {
  const meshRef = useRef<THREE.Mesh>(null);

  useFrame((_, delta) => {
    if (meshRef.current) {
      meshRef.current.rotation.y += delta * 0.8;
    }
  });

  const pos: [number, number, number] = [
    agent.position?.x ?? 0,
    (agent.position?.z ?? 0) + 15,
    agent.position?.y ?? 0,
  ];

  return (
    <group position={pos} onClick={(e) => { e.stopPropagation(); onSelect(); }}>
      <mesh ref={meshRef}>
        <octahedronGeometry args={[12, 0]} />
        <meshStandardMaterial
          color={isSelected ? "#00f0ff" : "#ffb703"}
          emissive={isSelected ? "#00f0ff" : "#ffb703"}
          emissiveIntensity={isSelected ? 0.8 : 0.3}
          wireframe={!isSelected}
        />
      </mesh>
      <Text
        position={[0, 20, 0]}
        fontSize={10}
        color={isSelected ? "#00f0ff" : "#c9d1d9"}
        anchorX="center"
        anchorY="middle"
      >
        {agent.agent_id}
      </Text>
    </group>
  );
}

export default function SpatialCanvas({
  agents,
  selectedAgentId,
  onSelectAgent,
  visualDensityScale,
}: SpatialCanvasProps) {
  const visibleAgents = React.useMemo(() => {
    if (visualDensityScale <= 0.3) {
      return agents.slice(0, Math.max(1, Math.floor(agents.length * 0.3)));
    }
    return agents;
  }, [agents, visualDensityScale]);

  return (
    <div className="relative w-full h-full bg-[#090d13]">
      <div className="absolute top-3 left-3 z-10 text-xs text-cyanGlow bg-[#161b22]/80 px-2 py-1 rounded border border-[#30363d]">
        3D Spatial Canvas | Visual Scale: {(visualDensityScale * 100).toFixed(0)}%
      </div>
      <Canvas camera={{ position: [0, 400, 600], fov: 45 }}>
        <ambientLight intensity={0.5} />
        <directionalLight position={[200, 500, 300]} intensity={1.2} />
        <TerrainMesh />
        {visibleAgents.map((a) => (
          <AgentMarker
            key={a.agent_id}
            agent={a}
            isSelected={a.agent_id === selectedAgentId}
            onSelect={() => onSelectAgent(a.agent_id)}
          />
        ))}
        <OrbitControls makeDefault enableDamping dampingFactor={0.05} maxPolarAngle={Math.PI / 2 - 0.05} />
      </Canvas>
    </div>
  );
}
