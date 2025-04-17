use bevy::prelude::*;
use tokio::runtime::Runtime;

#[derive(Resource)]
pub struct TokioRuntime(pub Runtime); 