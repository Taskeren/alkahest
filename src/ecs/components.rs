use destiny_pkg::TagHash;
use glam::Vec4;

use crate::{
    map_resources::MapResource,
    render::{scopes::ScopeRigidModel, ConstantBuffer, EntityRenderer},
    structure::ExtendedHash,
    types::AABB,
};

#[derive(Copy, Clone)]
/// Tiger entity world ID
pub struct EntityWorldId(pub u64);

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ResourceOriginType {
    Map,

    Activity,
    Activity2,
}

pub struct ResourcePoint {
    pub entity: ExtendedHash,
    pub resource_type: u32,
    pub resource: MapResource,

    pub has_havok_data: bool,
    /// Does this node belong to an activity?
    pub origin: ResourceOriginType,

    // TODO(cohae): Temporary
    pub entity_cbuffer: ConstantBuffer<ScopeRigidModel>,
}

impl ResourcePoint {
    pub fn entity_key(&self) -> u64 {
        match self.resource {
            MapResource::Unk80806aa3(_, t, _) => t.0 as u64,
            MapResource::Unk808068d4(t) => t.0 as u64,
            _ => self.entity.key(),
        }
    }
}

pub struct PointLight {
    pub attenuation: Vec4,
}

pub struct CubemapVolume(pub TagHash, pub AABB, pub String);

pub struct ActivityGroup(pub u32);

pub struct Label(pub String);

// TODO(cohae): This is currently only used by the spawn_entity_model command, should be used for all entity models for coherency sake
// TODO(cohae): use asset system hashes
pub struct EntityModel(pub EntityRenderer);
