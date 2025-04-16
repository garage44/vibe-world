use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use image::DynamicImage;
use reqwest::Client;
use std::time::Duration;
use crate::MAX_TILE_INDEX;

// Constants for the OSM tile system
const TILE_SIZE: usize = 256; // Standard OSM tile size in pixels

pub struct OSMTile {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl OSMTile {
    pub fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }

    pub fn get_url(&self) -> String {
        // Use the standard OSM tile server
        // The URL format is zoom/x/y where:
        // - x increases from west to east (0 to 2^zoom-1)
        // - y increases from north to south (0 to 2^zoom-1)
        format!(
            "https://a.tile.openstreetmap.org/{}/{}/{}.png",
            self.z, self.x, self.y
        )
    }
}

impl Clone for OSMTile {
    fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

pub async fn load_tile_image(tile: &OSMTile) -> Result<DynamicImage, anyhow::Error> {
    // Create a client with proper user agent and timeout
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("bevy_osm_viewer/0.1.0 (github.com/user/bevy_osm_viewer)")
        .build()?;

    let url = tile.get_url();
    info!("Requesting OSM tile URL: {}", url);

    // Attempt to load the tile with better error handling
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        error!("Failed to load tile {},{} - HTTP status: {}", tile.x, tile.y, response.status());
        return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
    }

    let bytes = response.bytes().await?;
    info!("Received {} bytes for tile {},{}", bytes.len(), tile.x, tile.y);

    let image = image::load_from_memory(&bytes)?;
    info!("Image loaded: {}x{}", image.width(), image.height());

    Ok(image)
}

// Create a tile mesh with the loaded image
pub fn create_tile_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    tile: &OSMTile,
    image: DynamicImage,
) -> Entity {
    // Create a custom mesh for a horizontal tile (XZ plane with Y as up)
    let mut mesh = Mesh::new(
        bevy::render::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    // Correct orientation for OSM tile mapping:
    // - OSM has (0,0) at the northwest corner
    // - X increases eastward (right)
    // - Y increases southward (down)
    // In our world coordinates:
    // - X increases eastward (same as OSM)
    // - Z increases southward (corresponds to OSM Y)
    // - Y is up (height)

    // Create vertices at exact [0,1] range to ensure perfect alignment
    let vertices: [[f32; 8]; 4] = [
        // positions (XYZ)               normals (XYZ)       UV coords
        [0.0, 0.0, 0.0,    0.0, 1.0, 0.0,          0.0, 0.0], // northwest corner
        [1.0, 0.0, 0.0,    0.0, 1.0, 0.0,          1.0, 0.0], // northeast corner
        [1.0, 0.0, 1.0,    0.0, 1.0, 0.0,          1.0, 1.0], // southeast corner
        [0.0, 0.0, 1.0,    0.0, 1.0, 0.0,          0.0, 1.0], // southwest corner
    ];

    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();
    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[3], v[4], v[5]]).collect();
    let uvs: Vec<[f32; 2]> = vertices.iter().map(|v| [v[6], v[7]]).collect();
    let indices = vec![0, 1, 2, 0, 2, 3]; // triangulate the quad

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

    // Check if we need to flip the image vertically to match the UV coordinates
    // OSM tiles have (0,0) at the top-left
    let flipped_image = image::DynamicImage::ImageRgba8(image.to_rgba8());
    let texture = Image::from_dynamic(flipped_image, true, RenderAssetUsages::default());
    let texture_handle = images.add(texture);

    // Create a material with the texture
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle),
        unlit: true, // Make the material unlit so it's always visible regardless of lighting
        alpha_mode: AlphaMode::Blend, // Enable transparency
        double_sided: true, // Make the material visible from both sides
        cull_mode: None,
        reflectance: 0.0, // No reflections to see the texture directly
        metallic: 0.0,    // No metallic effect to see the texture directly
        perceptual_roughness: 1.0, // No specular highlights
        ..default()
    });

    // CRITICAL: The key fix is in how we position tiles in world space
    // OSM has Y=0 at north, increasing southward
    // Our world has Z=0 at north, increasing southward
    commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::from_xyz(
                tile.x as f32,       // X coordinate (eastward)
                0.0,                 // At ground level
                tile.y as f32        // Direct mapping of OSM Y to world Z (southward)
            ),
            // Add name for debugging
            Name::new(format!("Tile {},{}", tile.x, tile.y)),
        ))
        .id()
}

// Create a fallback tile mesh for when the image can't be loaded
pub fn create_fallback_tile_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tile: &OSMTile,
) -> Entity {
    // Create a custom mesh for a horizontal tile (XZ plane with Y as up)
    let mut mesh = Mesh::new(
        bevy::render::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    // Match the new vertex positioning from create_tile_mesh
    let vertices: [[f32; 8]; 4] = [
        // positions (XYZ)               normals (XYZ)       UV coords
        [0.0, 0.0, 0.0,    0.0, 1.0, 0.0,          0.0, 0.0], // northwest corner
        [1.0, 0.0, 0.0,    0.0, 1.0, 0.0,          1.0, 0.0], // northeast corner
        [1.0, 0.0, 1.0,    0.0, 1.0, 0.0,          1.0, 1.0], // southeast corner
        [0.0, 0.0, 1.0,    0.0, 1.0, 0.0,          0.0, 1.0], // southwest corner
    ];

    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();
    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[3], v[4], v[5]]).collect();
    let uvs: Vec<[f32; 2]> = vertices.iter().map(|v| [v[6], v[7]]).collect();
    let indices = vec![0, 1, 2, 0, 2, 3]; // triangulate the quad

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

    // Create a checkered pattern material to indicate missing tile
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.2, 0.2), // Red color for missing tiles
        emissive: LinearRgba::new(0.5, 0.1, 0.1, 0.5), // Slight glow
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true, // Make the material visible from both sides
        cull_mode: None,
        ..default()
    });

    // Spawn the fallback tile entity with same positioning as regular tiles
    commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::from_xyz(
                tile.x as f32,       // X coordinate (eastward)
                0.0,                 // At ground level
                tile.y as f32        // Direct mapping of OSM Y to world Z (southward)
            ),
            // Add name for debugging
            Name::new(format!("Fallback Tile {},{}", tile.x, tile.y)),
        ))
        .id()
}
