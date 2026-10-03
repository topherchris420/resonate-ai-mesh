//! Spatial index of committed positions.
//!
//! Entities are stored in a `BTreeMap`, so iteration order (and therefore any
//! hash or listing derived from it) is identical on every run.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    pub const ZERO: Vector3 = Vector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn distance(&self, other: &Vector3) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialIndex {
    entities: BTreeMap<String, SpatialEntity>,
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert_entity(&mut self, entity: SpatialEntity) {
        self.entities.insert(entity.entity_id.clone(), entity);
    }

    pub fn get_entity(&self, id: &str) -> Option<&SpatialEntity> {
        self.entities.get(id)
    }

    /// Entities in ascending id order.
    pub fn list_entities(&self) -> Vec<SpatialEntity> {
        self.entities.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Entities within `radius` of `center`, in ascending id order.
    pub fn find_entities_in_radius(&self, center: &Vector3, radius: f64) -> Vec<SpatialEntity> {
        self.entities
            .values()
            .filter(|entity| entity.position.distance(center) <= radius)
            .cloned()
            .collect()
    }

    /// Nearest other entity to `center`, excluding `exclude_id`.
    pub fn nearest_other(
        &self,
        center: &Vector3,
        exclude_id: &str,
    ) -> Option<(&SpatialEntity, f64)> {
        self.entities
            .values()
            .filter(|entity| entity.entity_id != exclude_id)
            .map(|entity| (entity, entity.position.distance(center)))
            .min_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.0.entity_id.cmp(&right.0.entity_id))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: &str, x: f64, z: f64) -> SpatialEntity {
        SpatialEntity {
            entity_id: id.into(),
            position: Vector3::new(x, 0.0, z),
            orientation: Vector3::ZERO,
            velocity: Vector3::ZERO,
            terrain_reference: "arena".into(),
            coordinate_system: "local_sim".into(),
            timestamp: 1000,
            confidence: 0.99,
        }
    }

    #[test]
    fn spatial_index_operations() {
        let mut idx = SpatialIndex::new();
        idx.upsert_entity(entity("unit_01", 10.0, 10.0));
        assert_eq!(idx.get_entity("unit_01").unwrap().position.x, 10.0);
        assert_eq!(
            idx.find_entities_in_radius(&Vector3::new(10.0, 0.0, 0.0), 15.0)
                .len(),
            1
        );
        assert!(idx
            .find_entities_in_radius(&Vector3::new(500.0, 0.0, 0.0), 15.0)
            .is_empty());
    }

    #[test]
    fn listing_order_is_stable() {
        let mut idx = SpatialIndex::new();
        for id in ["c", "a", "b"] {
            idx.upsert_entity(entity(id, 0.0, 0.0));
        }
        let ids: Vec<String> = idx
            .list_entities()
            .into_iter()
            .map(|e| e.entity_id)
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn nearest_other_excludes_self_and_breaks_ties_by_id() {
        let mut idx = SpatialIndex::new();
        idx.upsert_entity(entity("self", 0.0, 0.0));
        idx.upsert_entity(entity("b", 5.0, 0.0));
        idx.upsert_entity(entity("a", -5.0, 0.0));
        let (nearest, distance) = idx.nearest_other(&Vector3::ZERO, "self").unwrap();
        assert_eq!(nearest.entity_id, "a");
        assert_eq!(distance, 5.0);
    }
}
