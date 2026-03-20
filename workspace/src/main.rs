//! Tank Tower Defense Game
//!
//! A tower defense game where players place turrets to defend against waves of enemy tanks.

use bevy::prelude::*;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};

mod resources;
mod states;

use resources::*;
use states::*;

fn main() {
    App::new()
        // Add default plugins with window configuration
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Tank Tower Defense".to_string(),
                resolution: (1024.0, 768.0).into(),
                resizable: true,
                ..default()
            }),
            ..default()
        }))
        // Add frame time diagnostics for FPS display
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        // Add state management
        .init_state::<GameState>()
        // Insert game resources
        .init_resource::<GameConfig>()
        .init_resource::<PlayerResources>()
        .init_resource::<WaveManager>()
        .init_resource::<GameStatistics>()
        // Add startup system
        .add_systems(Startup, setup)
        // Add state transition systems
        .add_systems(Update, handle_state_transitions)
        // Add menu systems
        .add_systems(OnEnter(GameState::Menu), menu_setup)
        .add_systems(Update, menu_update.run_if(in_state(GameState::Menu)))
        .add_systems(OnExit(GameState::Menu), menu_cleanup)
        // Add playing systems
        .add_systems(OnEnter(GameState::Playing), playing_setup)
        .add_systems(Update, (
            update_game_time,
            display_fps,
            handle_playing_input,
        ).run_if(in_state(GameState::Playing)))
        .add_systems(OnExit(GameState::Playing), playing_cleanup)
        // Add paused systems
        .add_systems(OnEnter(GameState::Paused), paused_setup)
        .add_systems(Update, handle_paused_input.run_if(in_state(GameState::Paused)))
        .add_systems(OnExit(GameState::Paused), paused_cleanup)
        // Add game over systems
        .add_systems(OnEnter(GameState::GameOver), game_over_setup)
        .add_systems(Update, handle_game_over_input.run_if(in_state(GameState::GameOver)))
        .add_systems(OnExit(GameState::GameOver), game_over_cleanup)
        .run();
}

/// Game configuration resource
#[derive(Resource, Debug, Clone)]
pub struct GameConfig {
    /// Target FPS for the game
    pub target_fps: u64,
    /// Tile size in pixels
    pub tile_size: f32,
    /// Map width in tiles
    pub map_width: u32,
    /// Map height in tiles
    pub map_height: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            tile_size: 32.0,
            map_width: 20,
            map_height: 15,
        }
    }
}

/// Resource to track elapsed game time
#[derive(Resource, Debug, Default, Deref, DerefMut)]
pub struct GameTime(pub f32);

/// Setup system that runs once at startup
fn setup(mut commands: Commands, config: Res<GameConfig>) {
    // Spawn a 2D camera
    commands.spawn((Camera2d::default(), Transform::from_xyz(
        config.map_width as f32 * config.tile_size / 2.0,
        config.map_height as f32 * config.tile_size / 2.0,
        0.0,
    )));

    // Initialize game time
    commands.insert_resource(GameTime::default());

    // Log startup info
    info!(
        "Tank Tower Defense initialized - Map: {}x{} tiles, Tile size: {}px",
        config.map_width, config.map_height, config.tile_size
    );
}

/// System to update game time each frame
fn update_game_time(time: Res<Time>, mut game_time: ResMut<GameTime>) {
    **game_time += time.delta_secs();
}

/// System to display FPS in the console periodically
fn display_fps(time: Res<Time>, diagnostics: Res<DiagnosticsStore>) {
    // Display FPS every 5 seconds
    let elapsed = time.elapsed_secs();
    if elapsed > 0.0 && (elapsed as u64) % 5 == 0 && (elapsed - elapsed.floor()).abs() < 0.1 {
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(smoothed) = fps.smoothed() {
                info!("FPS: {:.1}", smoothed);
            }
        }
    }
}