//! The window's icon (title bar and task bar): the HUD's resurrection dot, drawn in code
//! (`icon_dot.rs`, shared with build.rs which puts the same dot into the exe as its icon
//! resource for Explorer), set on the winit window once it exists.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[path = "icon_dot.rs"]
mod icon_dot;
use icon_dot::dot;

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_icon);
    }
}

/// bevy_winit 0.19 keeps the winit windows in a thread-local (`WINIT_WINDOWS`, main thread),
/// not a `NonSend` resource, so this borrows it the way bevy_winit's own systems do: the
/// `NonSendMarker` pins the system to the main thread (on a worker thread the map is empty).
fn set_icon(mut done: Local<bool>, primary: Query<Entity, With<PrimaryWindow>>, _main_thread: bevy::ecs::system::NonSendMarker) {
    if *done {
        return;
    }
    let Ok(entity) = primary.single() else { return };
    let size = 64;
    let Ok(icon) = winit::window::Icon::from_rgba(dot(size), size, size) else {
        *done = true;
        return;
    };
    bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
        if let Some(window) = windows.get_window(entity) {
            window.set_window_icon(Some(icon));
            info!("window icon set");
            *done = true;
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_dot_is_a_red_centre_in_a_dark_disk_with_a_clear_corner() {
        let size = 32;
        let px = super::dot(size);
        assert_eq!(px.len(), (size * size * 4) as usize);
        let at = |x: u32, y: u32| &px[((y * size + x) * 4) as usize..((y * size + x) * 4 + 4) as usize];
        assert_eq!(at(0, 0)[3], 0, "corner transparent");
        let centre = at(size / 2, size / 2);
        assert!(centre[0] > 180 && centre[1] < 60 && centre[3] == 255, "centre red: {centre:?}");
        let ring = at(size / 2, size / 2 + 10);
        assert!(ring[0] < 60 && ring[3] == 255, "lacquer ring: {ring:?}");
    }
}
