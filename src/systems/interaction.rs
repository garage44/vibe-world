use bevy::prelude::*;
use crate::resources::{OSMData, PersistentIslandSettings};
use crate::components::{TileCoords, PersistentIsland};
use crate::utils::coordinate_conversion::world_to_tile_coords;
use crate::resources::constants::PERSISTENT_ISLAND_ZOOM_LEVEL;

/// System to handle user interaction with persistent islands
pub fn interact_with_islands(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut island_settings: ResMut<PersistentIslandSettings>,
    mut osm_data: ResMut<OSMData>,
    camera_query: Query<(&Transform, &Camera), With<Camera3d>>,
    tiles_query: Query<(Entity, &TileCoords, &Transform, Option<&PersistentIsland>)>,
    mut materials_query: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Toggle highlight mode with H key
    if keyboard_input.just_pressed(KeyCode::KeyH) {
        island_settings.highlight_islands = !island_settings.highlight_islands;
        info!("Island highlight mode: {}", if island_settings.highlight_islands { "ON" } else { "OFF" });
    }
    
    // Toggle island editing mode with I key
    if keyboard_input.just_pressed(KeyCode::KeyI) {
        island_settings.editing_mode = !island_settings.editing_mode;
        info!("Island editing mode: {}", if island_settings.editing_mode { "ON" } else { "OFF" });
    }
    
    // Only handle island creation/deletion in editing mode
    if island_settings.editing_mode {
        // Create/delete islands with mouse click while in editing mode
        if mouse_input.just_pressed(MouseButton::Left) && keyboard_input.pressed(KeyCode::ShiftLeft) {
            if let Ok((camera_transform, _camera)) = camera_query.get_single() {
                // Get camera position and direction
                let ray_origin = camera_transform.translation;
                let ray_direction = camera_transform.forward();
                
                // Simple ray-plane intersection to find which tile we're looking at
                // Assume tiles are on the y=0 plane
                let t = -ray_origin.y / ray_direction.y;
                if t > 0.0 {
                    let hit_point = ray_origin + ray_direction * t;
                    info!("Ray hit ground at position: {:?}", hit_point);
                    
                    // Convert hit point to tile coordinates at the persistent island zoom level
                    let (tile_x, tile_y) = world_to_tile_coords(hit_point.x, hit_point.z, PERSISTENT_ISLAND_ZOOM_LEVEL);
                    info!("Target tile at zoom level {}: {}, {}", PERSISTENT_ISLAND_ZOOM_LEVEL, tile_x, tile_y);
                    
                    // Check if this is already a persistent island
                    if osm_data.persistent_islands.contains_key(&(tile_x, tile_y)) {
                        // Remove it
                        osm_data.persistent_islands.remove(&(tile_x, tile_y));
                        info!("Removed persistent island at {}, {}", tile_x, tile_y);
                        
                        // Instead of removing the component, update the tile to appear normal
                        // This approach prevents flickering as we don't destroy and recreate the tile
                        for (entity, coords, _, island) in tiles_query.iter() {
                            if coords.x == tile_x && coords.y == tile_y && coords.zoom == PERSISTENT_ISLAND_ZOOM_LEVEL {
                                if island.is_some() {
                                    // Just remove the PersistentIsland component, but keep the tile
                                    commands.entity(entity).remove::<PersistentIsland>();
                                    
                                    // We can't modify the material here directly, as we'd need to handle it
                                    // through the apply_pending_tiles system later
                                }
                                break;
                            }
                        }
                    } else {
                        // Add a new persistent island
                        let island_name = format!("Island_{},{}", tile_x, tile_y);
                        osm_data.persistent_islands.insert((tile_x, tile_y), PersistentIsland {
                            name: island_name.clone(),
                        });
                        info!("Created persistent island '{}' at {}, {}", island_name, tile_x, tile_y);
                        
                        // Find and add the persistent island component to any matching tile entity
                        let mut found_existing_tile = false;
                        for (entity, coords, _, _) in tiles_query.iter() {
                            if coords.x == tile_x && coords.y == tile_y && coords.zoom == PERSISTENT_ISLAND_ZOOM_LEVEL {
                                // Add the PersistentIsland component without recreating the tile
                                commands.entity(entity).insert(PersistentIsland {
                                    name: island_name.clone(),
                                });
                                
                                // We don't modify the material here, as the darkening effect
                                // will be applied in the next frame by the apply_pending_tiles system
                                found_existing_tile = true;
                                break;
                            }
                        }
                        
                        if !found_existing_tile {
                            info!("Note: Island created but tile not yet loaded. It will get the island status when loaded.");
                        }
                    }
                } else {
                    info!("Ray didn't hit ground plane - make sure you're looking at the ground");
                }
            } else {
                info!("Camera not found!");
            }
        }
    }
} 