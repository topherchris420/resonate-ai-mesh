use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpatialEntity {
    pub entity_id: String,
    pub position: Vector3,
    pub orientation: Vector3,
    pub velocity: Vector3,
    pub terrain_reference: String,
    pub coordinate_system: String,
    pub timestamp: i64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SpatialIndex {
    entities: HashMap<String, SpatialEntity>,
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    pub fn upsert_entity(&mut self, entity: SpatialEntity) {
        self.entities.insert(entity.entity_id.clone(), entity);
    }

    pub fn get_entity(&self, id: &str) -> Option<&SpatialEntity> {
        self.entities.get(id)
    }

    pub fn list_entities(&self) -> Vec<SpatialEntity> {
        self.entities.values().cloned().collect()
    }

    pub fn find_entities_in_radius(&self, center: &Vector3, radius: f64) -> Vec<SpatialEntity> {
        self.entities
            .values()
            .filter(|e| {
                let dx = e.position.x - center.x;
                let dy = e.position.y - center.y;
                let dz = e.position.z - center.z;
                (dx * dx + dy * dy + dz * dz).sqrt() <= radius
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spatial_index_operations() {
        let mut idx = SpatialIndex::new();
        let entity = SpatialEntity {
            entity_id: "unit_01".into(),
            position: Vector3 { x: 10.0, y: 0.0, z: 10.0 },
            orientation: Vector3::default(),
            velocity: Vector3::default(),
            terrain_reference: "lop_nur_sector_1".into(),
            coordinate_system: "ECEF".into(),
            timestamp: 1000,
            confidence: 0.99,
        };

        idx.upsert_entity(entity.clone());
        assert_eq!(idx.get_entity("unit_01"), Some(&entity));

        let nearby = idx.find_entities_in_radius(&Vector3 { x: 10.0, y: 0.0, z: 0.0 }, 15.0);
        assert_eq!(nearby.len(), 1);

        let far = idx.find_entities_in_radius(&Vector3 { x: 500.0, y: 0.0, z: 0.0 }, 15.0);
        assert_eq!(far.len(), 0);
    }
}
