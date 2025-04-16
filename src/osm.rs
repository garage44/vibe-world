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
    
    // When we create a mesh on the XZ plane in a 3D world:
    // - X increases from left to right (west to east)
    // - Z increases from back to front (north to south)
    // In OpenStreetMap:
    // - X increases from left to right (west to east)
    // - Y increases from top to bottom (north to south)
    // So our Z axis maps to OSM Y axis
    
    // Create a 1x1 quad centered at local origin (0,0,0)
    // This ensures tiles exactly touch each other when positioned at integer coordinates
    let vertices: [[f32; 8]; 4] = [
        // positions (XYZ)               normals (XYZ)       UV coords
        [-0.5, 0.0, -0.5,   0.0, 1.0, 0.0,          1.0, 1.0], // top-left → (1,1) UV - flipped
        [0.5, 0.0, -0.5,    0.0, 1.0, 0.0,          0.0, 1.0], // top-right → (0,1) UV - flipped
        [0.5, 0.0, 0.5,     0.0, 1.0, 0.0,          0.0, 0.0], // bottom-right → (0,0) UV - flipped
        [-0.5, 0.0, 0.5,    0.0, 1.0, 0.0,          1.0, 0.0], // bottom-left → (1,0) UV - flipped
    ];

    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();
    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[3], v[4], v[5]]).collect();
    let uvs: Vec<[f32; 2]> = vertices.iter().map(|v| [v[6], v[7]]).collect();
    let indices = vec![0, 1, 2, 0, 2, 3]; // triangulate the quad

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

    // We're now using flipped UVs instead of flipping the image
    // Create a texture from the loaded image directly
    let texture = Image::from_dynamic(image, true, RenderAssetUsages::default());
    let texture_handle = images.add(texture);
    
    // Create a material with the texture - ensure textures are visible
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

    // Spawn the tile entity
    // In OSM, Y increases southward (down), but we need to invert this for our 3D world
    commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            // Fix the tile positioning by ensuring exact integer coordinates
            // Important: We don't add 0.5 offset to coordinates anymore, since the mesh is
            // already centered at its local origin
            Transform::from_xyz(
                tile.x as f32, // X position matches OSM X directly
                0.0,           // At ground level
                // Lower Y value in OSM = Higher Z value in world (north is up)
                (MAX_TILE_INDEX - tile.y) as f32  // Invert Y to fix north/south orientation
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
    
    // Match the UV mapping from the main tile creation function
    let vertices: [[f32; 8]; 4] = [
        // positions (XYZ)               normals (XYZ)       UV coords
        [-0.5, 0.0, -0.5,   0.0, 1.0, 0.0,          1.0, 1.0], // top-left → (1,1) UV - flipped
        [0.5, 0.0, -0.5,    0.0, 1.0, 0.0,          0.0, 1.0], // top-right → (0,1) UV - flipped
        [0.5, 0.0, 0.5,     0.0, 1.0, 0.0,          0.0, 0.0], // bottom-right → (0,0) UV - flipped
        [-0.5, 0.0, 0.5,    0.0, 1.0, 0.0,          1.0, 0.0], // bottom-left → (1,0) UV - flipped
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

    // Spawn the fallback tile entity with the same positioning logic as main tiles
    commands
        .spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            // Fix the tile positioning to match the regular tiles
            Transform::from_xyz(
                tile.x as f32, // X position matches OSM X directly
                0.0,           // At ground level
                // Lower Y value in OSM = Higher Z value in world (north is up)
                (MAX_TILE_INDEX - tile.y) as f32  // Invert Y to fix north/south orientation
            ),
            // Add name for debugging
            Name::new(format!("Fallback Tile {},{}", tile.x, tile.y)),
        ))
        .with_children(|parent| {
            // Add a small cube on top to make it more visible
            parent.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.1, 0.1, 0.1).mesh())),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 1.0, 0.0), // Yellow
                    emissive: LinearRgba::new(1.0, 1.0, 0.0, 0.8),
                    ..default()
                })),
                Transform::from_xyz(0.0, 0.1, 0.0),
            ));
        })
        .id()
}
