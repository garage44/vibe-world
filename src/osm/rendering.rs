use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use image::DynamicImage;
use crate::osm::tile::OSMTile;
use crate::resources::constants::DEFAULT_ZOOM_LEVEL;
use crate::components::{TileCoords, BackgroundTile};

// Bundle for the tile entity to ensure all components are added atomically
#[derive(Bundle)]
struct TileBundle {
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
    transform: Transform,
    global_transform: GlobalTransform,
    name: Name,
}

// Create a tile mesh with the loaded image
pub fn create_tile_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    tile: &OSMTile,
    image: DynamicImage,
    current_time: f32,
    is_background: bool,
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
    
    // PERFORMANCE: Create texture with optimal rendering settings
    let texture = Image::from_dynamic(flipped_image, true, RenderAssetUsages::default());
    let texture_handle = images.add(texture);

    // FIX VISIBILITY: Return to AlphaMode::Blend if the OSM tiles have transparency
    // Many map tiles have transparent areas that need to be rendered properly
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle),
        unlit: true, // Make the material unlit for better performance
        alpha_mode: AlphaMode::Blend, // Changed back to Blend to support transparency in tiles
        // Important: Double-sided rendering is needed for map tiles visibility
        double_sided: true,
        cull_mode: None,
        // Performance: Minimal material properties
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    });

    // Calculate zoom level difference to determine scaling and positioning
    let zoom_difference = tile.z as i32 - DEFAULT_ZOOM_LEVEL as i32;
    let scale_factor = 2_f32.powi(-zoom_difference); // Inverse because higher zoom = smaller tile

    // Create mesh and material handles
    let mesh_handle = meshes.add(mesh);
    let material_handle = material;

    // Calculate y-offset based on zoom level to handle z-fighting
    // Higher zoom levels (more detailed) should be higher up
    // Use a small offset that won't be noticeable visually but will fix z-fighting
    let y_offset = if is_background {
        // Background tiles should always be below focus tiles
        -0.01
    } else {
        // Higher zoom levels should be on top
        0.005 * (tile.z as f32 / 19.0) // Normalize to a small range
    };

    // Create transform
    let transform = Transform::from_xyz(
        tile.x as f32 * scale_factor,       // Scale X coordinate
        y_offset,                          // Small Y offset based on zoom to prevent z-fighting
        tile.y as f32 * scale_factor        // Scale Z coordinate
    )
    .with_scale(Vec3::new(scale_factor, 1.0, scale_factor)); // Scale the tile size

    // Spawn entity with everything at once
    let mut entity_builder = commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
        GlobalTransform::default(),
        Name::new(format!("Tile {},{}, zoom {}", tile.x, tile.y, tile.z)),
        TileCoords {
            x: tile.x,
            y: tile.y,
            zoom: tile.z,
            last_used: current_time,
        },
    ));
    
    // Add background component if this is a background tile
    if is_background {
        entity_builder.insert(BackgroundTile);
    }
    
    entity_builder.id()
}

// Create a fallback tile mesh for when the image can't be loaded
pub fn create_fallback_tile_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tile: &OSMTile,
    current_time: f32,
    is_background: bool,
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

    // FIX VISIBILITY: Change fallback material to be more visible
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.2, 0.2), // Red color for missing tiles
        alpha_mode: AlphaMode::Opaque, // Make opaque for better performance
        unlit: true, // Unlit for better performance
        // Important: Double-sided rendering is needed for map tiles visibility
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Calculate zoom level difference to determine scaling and positioning
    let zoom_difference = tile.z as i32 - DEFAULT_ZOOM_LEVEL as i32;
    let scale_factor = 2_f32.powi(-zoom_difference); // Inverse because higher zoom = smaller tile

    // Create mesh and material handles
    let mesh_handle = meshes.add(mesh);
    let material_handle = material;

    // Calculate y-offset based on zoom level to handle z-fighting
    // Higher zoom levels (more detailed) should be higher up
    // Use a small offset that won't be noticeable visually but will fix z-fighting
    let y_offset = if is_background {
        // Background tiles should always be below focus tiles
        -0.01
    } else {
        // Higher zoom levels should be on top
        0.005 * (tile.z as f32 / 19.0) // Normalize to a small range
    };

    // Create transform
    let transform = Transform::from_xyz(
        tile.x as f32 * scale_factor,     // Scale X coordinate
        y_offset,                        // Small Y offset based on zoom to prevent z-fighting
        tile.y as f32 * scale_factor      // Scale Z coordinate
    )
    .with_scale(Vec3::new(scale_factor, 1.0, scale_factor)); // Scale the tile size

    // Spawn entity with everything at once
    let mut entity_builder = commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
        GlobalTransform::default(),
        Name::new(format!("Fallback Tile {},{}, zoom {}", tile.x, tile.y, tile.z)),
        TileCoords {
            x: tile.x,
            y: tile.y,
            zoom: tile.z,
            last_used: current_time,
        },
    ));
    
    // Add background component if this is a background tile
    if is_background {
        entity_builder.insert(BackgroundTile);
    }
    
    entity_builder.id()
}

// Create a material with special highlighting for persistent islands
#[allow(dead_code)]
pub fn create_highlighted_material(
    _materials: &mut Assets<StandardMaterial>,
    texture_handle: Handle<Image>,
    highlight_color: Color,
) -> StandardMaterial {
    // Create a material with highlighting for islands
    StandardMaterial {
        base_color_texture: Some(texture_handle),
        base_color: highlight_color, // Apply a tint
        unlit: false, // Keep lighting enabled to show the tint
        alpha_mode: AlphaMode::Blend, // Enable transparency
        double_sided: true, // Make the material visible from both sides
        cull_mode: None,
        reflectance: 0.0, // No reflections to see the texture directly
        metallic: 0.0,    // No metallic effect to see the texture directly
        perceptual_roughness: 1.0, // No specular highlights
        ..default()
    }
} 