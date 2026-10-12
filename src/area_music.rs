//! Area battle music (docs/kb/sound.md "Area battle music"): the game's common.emevd event 410.
//! The engine keeps a map music state each frame (WorldSoundManImp +0x11fc, FUN_1408aa950: the
//! highest any character asked for this frame, FUN_1408aa5c0): an enemy asks for 3 (combat) in
//! AI state 2 / 3, for 1 ("about to be found") in AI state 1, 2 ("about to lose you") when that
//! state carries its search goal, 0 when dead (CSChrSoundModule byte, FUN_140a04760 /
//! FUN_140a05310; HKS act(3014)). Event 410 turns on the map's MSB BGM region for the state
//! (2010[8]): combat <area><block>2680, state 1 2688, state 2 2689 (tools/area_bgm.py ->
//! extracted/enemies/area_bgm.json). When combat ends it stops all music (fade-out 0.7 s) and
//! holds 2 s in which only combat may start again, then up to 1 s more that states 1 / 2 / 3 cut
//! short (the field track waits it out); states 1 and 2 swap without a gap.
//! Nothing plays while a boss's music does (flag 9200, fmod.rs `boss_music`).
//! With no other track due, 410 plays the area's outdoor field track (region 2690
//! "BGM_屋外フィールド_平常時", json "field") while flag 9100 is on, and fades all music out over
//! 1.7 s while it is off. common_func 20004010 keeps 9100 on while Wolf is outside every one of
//! the map's indoor-check regions (json "indoor", tools/area_bgm.py) and off inside any.
//! Gaps: our targeting has no search-goal flag, so state 2 never comes up; the story-raid
//! variants, 410 label 9's alternative field tracks (2691 / 2692 behind story flags and the
//! 1102695 / 1702695 regions) and the swim fade are not done; the 0.7 s / 1.7 s fade-outs are
//! FMOD's own event fade here.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::fmod::{EventHandle, Fmod};

/// One map's tracks (FMOD events "m<SoundID>" in `bank`).
#[derive(serde::Deserialize, Clone, Debug, Default)]
struct AreaTracks {
    bank: String,
    combat: String,
    #[serde(default)]
    found_soon: Option<String>,
    #[serde(default)]
    losing: Option<String>,
    /// The outdoor field track (region 2690).
    #[serde(default)]
    field: Option<String>,
    /// The indoor-check regions (flag 9100), in map space.
    #[serde(default)]
    indoor: Vec<Zone>,
}

/// One MSB region of tools/area_bgm.py "indoor": shape 2 sphere (radius), 3 cylinder (radius,
/// height), 5 box (width x, depth z, height y) turned by `yaw` degrees; boxes and cylinders stand
/// on `pos`.
#[derive(serde::Deserialize, Clone, Debug, Default)]
struct Zone {
    shape: u32,
    pos: [f32; 3],
    yaw: f32,
    size: [f32; 3],
}

impl Zone {
    fn contains(&self, p: Vec3) -> bool {
        let d = p - Vec3::from_array(self.pos);
        match self.shape {
            2 => d.length() <= self.size[0],
            3 => d.xz().length() <= self.size[0] && (0.0..=self.size[1]).contains(&d.y),
            5 => {
                // Into the box's frame (sekiro-extract msb::rotate about Y, undone).
                let (s, c) = (-self.yaw.to_radians()).sin_cos();
                let (x, z) = (d.x * c + d.z * s, -d.x * s + d.z * c);
                x.abs() <= self.size[0] * 0.5 && z.abs() <= self.size[1] * 0.5 && (0.0..=self.size[2]).contains(&d.y)
            }
            _ => false,
        }
    }
}

fn area_bgm() -> &'static HashMap<String, AreaTracks> {
    static BGM: std::sync::OnceLock<HashMap<String, AreaTracks>> = std::sync::OnceLock::new();
    BGM.get_or_init(|| {
        let path = crate::paths::extracted().join("enemies/area_bgm.json");
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    })
}

/// The track event 410 wants.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Track {
    Combat,
    FoundSoon,
    Losing,
    Field,
}

/// After combat music stops: 410 label 13 waits 2.0 s (or until combat resumes), then up to
/// 1.0 s more (or until state 1 / 2 / 3) before it restarts and may start another track.
const COMBAT_HOLD: f32 = 2.0;
const SETTLE: f32 = 1.0;

/// Event 410's choice over time.
#[derive(Default, Debug)]
pub(crate) struct Music {
    pub playing: Option<Track>,
    hold: f32,
}

impl Music {
    /// `state`: the map music state this frame (0 none, 1 about to be found, 2 about to lose
    /// you, 3 combat); `outdoors`: flag 9100. Returns the track that should play.
    pub(crate) fn step(&mut self, state: u8, outdoors: bool, dt: f32) -> Option<Track> {
        self.hold = (self.hold - dt).max(0.0);
        let want = match state {
            3 => Some(Track::Combat),
            1 => Some(Track::FoundSoon),
            2 => Some(Track::Losing),
            _ if outdoors => Some(Track::Field),
            _ => None,
        };
        if self.playing == Some(Track::Combat) && want != Some(Track::Combat) {
            self.playing = None;
            self.hold = COMBAT_HOLD + SETTLE;
        }
        let free = self.hold <= 0.0 || (self.hold <= SETTLE && state != 0);
        if free {
            // The wait was cut short: 410 has restarted.
            self.hold = 0.0;
        }
        if want == Some(Track::Combat) || free {
            self.playing = want;
        }
        self.playing
    }
}

/// An enemy's request (the CSChrSoundModule byte) from its target state: FIND / BATTLE (the AI's
/// alert / combat) -> 3, CAUTION -> 1, else 0; dead, disabled or parked -> 0.
fn music_state(e: &crate::enemy::Enemy, parked: bool) -> u8 {
    if e.is_dead() || e.is_disabled() || parked {
        return 0;
    }
    match e.targeting.state {
        s if s >= crate::stealth::FIND => 3,
        crate::stealth::CAUTION => 1,
        _ => 0,
    }
}

fn area_music(
    fmod: Option<NonSendMut<Fmod>>,
    map: Option<Res<crate::map::MapLighting>>,
    script: Option<Res<crate::enemy::BossScript>>,
    screen: Option<Res<State<crate::menu::Screen>>>,
    enemies: Query<(&crate::enemy::Enemy, Option<&crate::enemy::Phantom>)>,
    wolf: Query<&Transform, With<crate::player::Player>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    time: Res<Time>,
    mut music: Local<Music>,
    mut playing: Local<Option<(String, EventHandle)>>,
    mut last: Local<Option<(u8, bool, bool, bool)>>,
    mut from_fight: Local<bool>,
) {
    let Some(mut fmod) = fmod else { return };
    let fighting = crate::fmod::fight_screen(screen.as_ref().map(|s| s.get()), &mut from_fight);
    let boss = script.as_ref().and_then(|s| s.run.as_ref()).is_some_and(|r| r.defeated.is_none());
    let tracks = map.as_ref().and_then(|m| area_bgm().get(&m.source_map));
    let state = if fighting && !boss && tracks.is_some() { enemies.iter().map(|(e, ph)| music_state(e, ph.is_some_and(|p| p.parked()))).max().unwrap_or(0) } else { 0 };
    // Flag 9100: Wolf's feet outside every indoor-check region of the source map (only on a map
    // that passes 20004010 its regions).
    let outdoors = fighting
        && !boss
        && tracks.is_some_and(|t| {
            let at = wolf.single().ok().zip(map.as_ref()).and_then(|(w, m)| m.to_map(w.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT));
            !t.indoor.is_empty() && at.is_some_and(|p| !t.indoor.iter().any(|z| z.contains(p)))
        });
    if *last != Some((state, boss, tracks.is_some(), outdoors)) {
        *last = Some((state, boss, tracks.is_some(), outdoors));
        info!("area music: state {state}, outdoors {outdoors} (boss music {boss}, map {:?} has tracks {})", map.as_ref().map(|m| m.source_map.as_str()), tracks.is_some());
    }
    let track = music.step(state, outdoors, time.delta_secs());
    let want = tracks.zip(track).and_then(|(t, k)| match k {
        Track::Combat => Some(t.combat.as_str()),
        Track::FoundSoon => t.found_soon.as_deref(),
        Track::Losing => t.losing.as_deref(),
        Track::Field => t.field.as_deref(),
    });
    if playing.as_ref().map(|p| p.0.as_str()) != want {
        if let Some((key, ev)) = playing.take() {
            info!("FMOD: area music {key} off");
            fmod.stop(ev);
        }
        if let (Some(key), Some(t)) = (want, tracks) {
            fmod.load_project(&t.bank);
            let at = camera.single().map_or(Vec3::ZERO, |c| c.translation());
            if let Some(ev) = fmod.play_tracked(key, at) {
                info!("FMOD: area music {key} ({})", t.bank);
                *playing = Some((key.to_string(), ev));
            }
        }
    }
    if let (Some((_, ev)), Ok(c)) = (playing.as_ref(), camera.single()) {
        let ev = *ev;
        fmod.set_position(ev, c.translation());
    }
}

pub struct AreaMusicPlugin;

impl Plugin for AreaMusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, area_music);
    }
}

#[cfg(test)]
mod tests {
    use super::{Music, Track, Zone};
    use bevy::prelude::*;

    /// Combat starts at once; when it ends the music stops and only combat may start for 2 s,
    /// then state 1 / 2 may (the "about to be found" track); the two lighter tracks swap without
    /// a gap; the field track waits the whole 3 s.
    #[test]
    fn event_410_holds_after_combat() {
        let mut m = Music::default();
        assert_eq!(m.step(3, false, 0.1), Some(Track::Combat));
        assert_eq!(m.step(1, false, 0.1), None);
        assert_eq!(m.step(1, false, 1.0), None);
        assert_eq!(m.step(3, false, 0.1), Some(Track::Combat));
        assert_eq!(m.step(0, false, 0.1), None);
        assert_eq!(m.step(1, false, 1.0), None);
        assert_eq!(m.step(1, false, 0.5), None);
        assert_eq!(m.step(1, false, 0.6), Some(Track::FoundSoon));
        assert_eq!(m.step(2, false, 0.1), Some(Track::Losing));
        assert_eq!(m.step(0, false, 0.1), None);
        // Outdoors with nothing else due: the field track, after combat's 3 s too.
        assert_eq!(m.step(0, true, 0.1), Some(Track::Field));
        assert_eq!(m.step(3, true, 0.1), Some(Track::Combat));
        assert_eq!(m.step(0, true, 0.1), None);
        assert_eq!(m.step(0, true, 2.5), None);
        assert_eq!(m.step(0, true, 0.5), Some(Track::Field));
        assert_eq!(m.step(0, false, 0.1), None);
    }

    /// Boxes stand on their position and turn by their yaw; spheres and cylinders by radius.
    #[test]
    fn indoor_zones_contain() {
        let b = Zone { shape: 5, pos: [10.0, 5.0, 0.0], yaw: 90.0, size: [2.0, 8.0, 3.0] };
        // Turned 90 degrees: the 8 m depth runs along X.
        assert!(b.contains(Vec3::new(13.5, 5.1, 0.5)));
        assert!(!b.contains(Vec3::new(10.5, 5.1, 3.5)));
        assert!(!b.contains(Vec3::new(10.0, 4.9, 0.0)), "below the box's floor");
        assert!(b.contains(Vec3::new(10.0, 7.9, 0.0)));
        let s = Zone { shape: 2, pos: [0.0; 3], yaw: 0.0, size: [2.0, 0.0, 0.0] };
        assert!(s.contains(Vec3::new(0.0, -1.5, 0.0)) && !s.contains(Vec3::new(1.5, 1.5, 0.0)));
        let c = Zone { shape: 3, pos: [0.0; 3], yaw: 0.0, size: [2.0, 4.0, 0.0] };
        assert!(c.contains(Vec3::new(1.0, 3.0, 1.0)) && !c.contains(Vec3::new(0.0, -0.1, 0.0)));
    }

    /// arena_to_map undoes the export: an exported map's enemy stands 4 m behind the arena
    /// origin (Bevy (0, y, -4)), at its MSB position (json centre.pos).
    #[test]
    fn arena_points_back_to_map_space() {
        let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("map_m11_01_00_00.json")) else { return };
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let o = json["origin"].as_array().unwrap();
        let origin = Vec3::new(f(&o[0]), f(&o[1]), f(&o[2]));
        let c = json["centre"]["pos"].as_array().unwrap();
        let back = crate::map::arena_to_map(origin, f(&json["arena_yaw"]), Vec3::new(0.0, f(&c[1]) - origin.y, -4.0));
        assert!(back.distance(Vec3::new(f(&c[0]), f(&c[1]), f(&c[2]))) < 0.05, "{back} vs {c:?}");
    }
}
