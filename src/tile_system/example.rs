use bevy::{
    prelude::*,
    app::AppExit,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin, DiagnosticPath},
    text::{Text, TextSection, TextStyle},
    render::camera::{Camera3d, ScalingMode, OrthographicProjection, Projection, ClearColorConfig},
    sprite::SpriteBundle,
    transform::TransformBundle,
    window::PrimaryWindow,
};
use crate::tile_system::{
    types::*,
    scheduler::TileScheduler,
    loader::TileLoader,
    downloader::TileDownloadQueue,
    meshing::TileMesher,
    cache::TileCache,
};
use std::time::{Duration, Instant};

/// Example plugin for the tile system
pub struct TileSystemExamplePlugin;

impl Plugin for TileSystemExamplePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup_example)
            .add_systems(Update, (
                key_controls,
                update_ui,
            ));
    }
}

/// Component to track zoom for the camera
#[derive(Component)]
struct CameraZoom {
    scale: f32,
}

impl Default for CameraZoom {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// Set up the example scene
fn setup_example(
    mut commands: Commands,
    mut tile_loader: ResMut<crate::tile_system::loader::TileLoader>,
    asset_server: Res<AssetServer>,
) {
    // Set up the tile server URL
    tile_loader.set_tile_server("https://tile.openstreetmap.org".to_string());
    
    // Spawn a camera
    commands.spawn((
        Camera3dBundle {
            camera: Camera3d::default(),
            projection: Projection::Orthographic(OrthographicProjection {
                scale: 1.0,
                ..default()
            }),
            transform: Transform::from_xyz(0.0, 10.0, 0.0)
                .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            ..default()
        },
        crate::tile_system::CameraTransform,
        CameraZoom::default(),
    ));
    
    // Add a light
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(50.0, 50.0, 50.0)
            .looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
    
    // UI for displaying status
    commands
        .spawn(NodeBundle::default())
        .with_children(|parent| {
            // Add text for displaying information
            let text_style1 = TextStyle {
                font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                font_size: 24.0,
                color: Color::WHITE,
            };
            
            let text_style2 = TextStyle {
                font: asset_server.load("fonts/FiraSans-Regular.ttf"),
                font_size: 18.0,
                color: Color::WHITE,
            };
            
            parent.spawn((
                // In Bevy 0.15, create TextBundle differently
                TextBundle::default()
                    .with_text(Text::from_sections([
                        TextSection::new("Vibers Tile System Example\n", text_style1),
                        TextSection::new("Status: Initializing...\n", text_style2),
                    ])),
                ExampleUIText,
            ));
        });
}

/// Component to identify UI text
#[derive(Component)]
struct ExampleUIText;

/// Process keyboard input to control the camera
fn key_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<(&mut Transform, &mut CameraZoom, &mut Projection), With<Camera3d>>,
) {
    let Ok((mut transform, mut camera_zoom, mut projection)) = camera_query.get_single_mut() else {
        return;
    };
    
    // Get time since last frame
    let delta = time.delta_secs();
    
    // Movement speed (units per second)
    let mut speed = 500.0;
    
    // Faster movement with shift
    if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
        speed *= 3.0;
    }
    
    // Camera movement
    if keyboard.pressed(KeyCode::KeyW) {
        transform.translation.y += speed * delta * camera_zoom.scale;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        transform.translation.y -= speed * delta * camera_zoom.scale;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        transform.translation.x -= speed * delta * camera_zoom.scale;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        transform.translation.x += speed * delta * camera_zoom.scale;
    }
    
    // Zoom control
    if keyboard.pressed(KeyCode::KeyQ) {
        camera_zoom.scale *= 1.0 + delta;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        camera_zoom.scale *= 1.0 - delta;
    }
    
    // Reset view
    if keyboard.just_pressed(KeyCode::KeyR) {
        transform.translation = Vec3::new(0.0, 0.0, 0.0);
        camera_zoom.scale = 1.0;
    }
    
    // Clamp zoom level to reasonable values
    camera_zoom.scale = camera_zoom.scale.clamp(0.05, 10.0);
    
    // Update the orthographic projection scale
    if let Projection::Orthographic(ref mut ortho) = *projection {
        ortho.scale = camera_zoom.scale;
    }
}

/// Update the UI with status information
fn update_ui(
    mut query: Query<&mut Text, With<ExampleUIText>>,
    time: Res<Time>,
    tile_loader: Res<crate::tile_system::loader::TileLoader>,
    diagnostics: Res<DiagnosticsStore>,
) {
    let Ok(mut text) = query.get_single_mut() else {
        return;
    };

    // Only update UI text every 0.2 seconds
    if time.elapsed_secs() % 0.2 > 0.1 {
        return;
    }

    // Get FPS value from diagnostics
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or(0.0);

    // Update the second section of the text (status)
    // In Bevy 0.15, access sections directly from Text
    if let Some(section) = text.sections.get_mut(1) {
        section.value = format!(
            "Status: Running\n\
             FPS: {:.1}\n\
             Tiles downloading: {}\n\
             Tiles queued: {}\n\
             WASD: Move camera | Q/E: Zoom | R: Reset view",
            fps,
            tile_loader.active_download_count(),
            tile_loader.queued_download_count()
        );
    }
} 