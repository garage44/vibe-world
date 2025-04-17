use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use crate::tile_system::types::*;
use std::sync::Arc;

/// Material component for fading tiles
#[derive(Component)]
pub struct FadeMaterial {
    /// Current opacity (0-1)
    pub opacity: f32,
    /// Target opacity (0-1)
    pub target: f32,
    /// Fade speed (per second)
    pub speed: f32,
}

/// Create a mesh for a tile
pub fn create_tile_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    id: TileId,
    image: Arc<Image>,
    current_time: f32,
) -> Entity {
    // Create a mesh for the tile (flat quad in XZ plane with Y up)
    let mesh = create_quad_mesh();
    
    // Create material with the tile texture
    let image_handle = images.add((*image).clone());
    
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(image_handle),
        unlit: true, // Make the material unlit for better performance
        alpha_mode: AlphaMode::Blend, // Support transparency in tiles
        double_sided: true, // Make visible from both sides
        cull_mode: None,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    });
    
    // Calculate world position based on tile coordinates
    let (min, max) = id.bounds();
    // Calculate center position
    let center = Vec3::new(
        (min.x + max.x) / 2.0,
        0.0,
        (min.z + max.z) / 2.0
    );
    // Calculate scale
    let scale = Vec3::new(
        max.x - min.x,
        1.0,
        max.z - min.z
    );
    
    // Create the entity
    let mesh_handle = meshes.add(mesh);
    
    commands.spawn((
        PbrBundle {
            mesh: mesh_handle,
            material: material,
            transform: Transform::from_translation(center).with_scale(scale),
            ..default()
        },
        Name::new(format!("Tile {},{},{}", id.x, id.y, id.zoom)),
    )).id()
}

/// Create a fallback mesh for a tile that failed to load
pub fn create_fallback_tile_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    id: TileId,
    current_time: f32,
) -> Entity {
    // Create a mesh for the tile (flat quad in XZ plane with Y up)
    let mesh = create_quad_mesh();
    
    // Create a red material for missing tiles
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.2, 0.2), // Red color
        unlit: true,
        alpha_mode: AlphaMode::Opaque,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    
    // Calculate world position based on tile coordinates
    let (min, max) = id.bounds();
    // Calculate center position
    let center = Vec3::new(
        (min.x + max.x) / 2.0,
        0.0,
        (min.z + max.z) / 2.0
    );
    // Calculate scale
    let scale = Vec3::new(
        max.x - min.x,
        1.0,
        max.z - min.z
    );
    
    // Create the entity
    let mesh_handle = meshes.add(mesh);
    
    commands.spawn((
        PbrBundle {
            mesh: mesh_handle,
            material: material,
            transform: Transform::from_translation(center).with_scale(scale),
            ..default()
        },
        Name::new(format!("Fallback Tile {},{},{}", id.x, id.y, id.zoom)),
    )).id()
}

/// Create a standard quad mesh for tiles
fn create_quad_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        bevy::render::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    
    // Create a quad centered at the origin
    // The quad spans from -0.5 to 0.5 in X and Z
    let vertices: [[f32; 8]; 4] = [
        // Position (XYZ)           Normal (XYZ)       UV
        [-0.5, 0.0, -0.5,    0.0, 1.0, 0.0,    0.0, 0.0], // Top-left
        [0.5, 0.0, -0.5,     0.0, 1.0, 0.0,    1.0, 0.0], // Top-right
        [0.5, 0.0, 0.5,      0.0, 1.0, 0.0,    1.0, 1.0], // Bottom-right
        [-0.5, 0.0, 0.5,     0.0, 1.0, 0.0,    0.0, 1.0], // Bottom-left
    ];
    
    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();
    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[3], v[4], v[5]]).collect();
    let uvs: Vec<[f32; 2]> = vertices.iter().map(|v| [v[6], v[7]]).collect();
    let indices = vec![0, 1, 2, 0, 2, 3]; // Two triangles
    
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));
    
    mesh
}

/// Update material to reflect the fade status
pub fn update_fade_material(
    commands: &mut Commands,
    entity: Entity,
    materials: &mut Assets<StandardMaterial>,
    material_handle: &Handle<StandardMaterial>,
    alpha: f32,
) {
    if let Some(material) = materials.get_mut(material_handle) {
        // Update the base color alpha
        let mut color = material.base_color;
        material.base_color = color.with_a(alpha);
    }
}

/// System to update material alpha for fading tiles
pub fn update_fading_materials(
    mut materials: ResMut<Assets<StandardMaterial>>,
    fade_query: Query<(&Handle<StandardMaterial>, &FadeMaterial)>,
) {
    for (material_handle, fade) in fade_query.iter() {
        if let Some(material) = materials.get_mut(material_handle) {
            let color = material.base_color;
            material.base_color = color.with_a(fade.opacity);
        }
    }
}

/// Create a different style mesh
pub fn create_tile_mesh_hex(id: TileId) -> Mesh {
    create_quad_mesh()
} 