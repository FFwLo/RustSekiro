//! Skill experience: a defeated enemy gives its NpcParam `skillPoint` experience and `getSoul`
//! Sen; experience fills the gauge and each full gauge is a skill point to spend at an idol
//! (Acquire Skills, src/skill_tree.rs). Dying without a resurrection loses half the Sen and of
//! the experience not yet a point (TentativePlayerParam MoneyLostRate / SkillPointLostRate 50).
//!
//! The experience per point is not in the params (the exe computes it); `TO_NEXT` is the table
//! the Fextralife wiki measured (Skill Points and Experience): 500 for the first point, slowly
//! rising to the 25th, steeply after. gap: Unseen Aid (the chance to keep them) is not rolled.

use std::collections::HashSet;

use bevy::prelude::*;

/// Experience from skill point n to n + 1 (n = points earned so far, spent or not); past the
/// table the last step repeats.
const TO_NEXT: [u32; 128] = [
    500, 514, 528, 542, 557, 572, 587, 602, 618, 634, 650, 666, 682, 698, 715, 732, 749, 766, 784, 802, 820, 838, 856, 874, 893, //
    1093, 1300, 1515, 1738, 1970, 2210, 2458, 2715, 2980, 3254, 3538, 3830, 4131, 4442, 4762, 5092, 5431, 5780, 6139, 6507, 6886, 7276,
    7675, 8085, 8506, 8938, 9380, 9833, 10297, 10773, 11260, 11758, 12268, 12789, 13322, 13868, 14425, 14994, 15576, 16170, 16777,
    17396, 18028, 18673, 19331, 20002, 20686, 21383, 22094, 22819, 23558, 24310, 25076, 25856, 26651, 27460, 28283, 29121, 29973,
    30840, 31723, 32620, 33532, 34460, 35403, 36362, 37336, 38326, 39332, 40354, 41392, 42446, 43516, 44603, 45707, 46828, 47965,
    49119, 50290, 51479, 52685, 53908, 55149, 56407, 57683, 58977, 60290, 61620, 62969, 64336, 65722, 67126, 68549, 69991, 71452,
    72932, 74431, 75949, 77487, 79045, 80622, 82220, 83837,
];

/// Experience needed from `earned` points to the next.
pub(crate) fn to_next(earned: u32) -> u32 {
    TO_NEXT[(earned as usize).min(TO_NEXT.len() - 1)]
}

/// Adds experience; every full gauge becomes a skill point. Returns the points gained.
pub(crate) fn gain(p: &mut crate::config::PlayerConfig, exp: u32) -> u32 {
    p.skill_exp = p.skill_exp.saturating_add(exp);
    let mut points = 0;
    while p.skill_exp >= to_next(p.skill_level) {
        p.skill_exp -= to_next(p.skill_level);
        p.skill_level += 1;
        p.skill_points += 1;
        points += 1;
    }
    points
}

/// How full the gauge is (0..1).
pub(crate) fn fill(p: &crate::config::PlayerConfig) -> f32 {
    p.skill_exp as f32 / to_next(p.skill_level) as f32
}

/// An enemy's reward for its defeat: (experience, Sen) from its NpcParam row.
pub(crate) fn reward(npc: &serde_json::Value) -> (u32, u32) {
    let get = |k: &str| npc[k].as_i64().unwrap_or(0).max(0) as u32;
    (get("skillPoint"), get("getSoul"))
}

/// What kills have paid in all: the HUD shows the change as "+N" under the gauge and the purse.
#[derive(Resource, Default)]
pub(crate) struct Earned {
    pub(crate) exp: u32,
    pub(crate) sen: u32,
}

/// Pays each enemy once as it dies (again after it stands back up: a rest at an idol, a reset).
fn reward_kills(
    combat: Res<crate::data::Combat>,
    mut config: ResMut<crate::config::GameConfig>,
    mut earned: ResMut<Earned>,
    enemies: Query<(Entity, &crate::enemy::Enemy, &crate::actor::Actor)>,
    wolf: Query<&crate::player::Player>,
    mut paid: Local<HashSet<Entity>>,
) {
    // The Mibu Balloon of Wealth's soulRate (items.rs).
    let rate = wolf.single().map_or(1.0, |p| crate::items::sen_rate(&p.timed.iter().map(|t| t.0).collect::<Vec<_>>()));
    paid.retain(|e| enemies.get(*e).is_ok_and(|(_, en, _)| en.is_dead()));
    for (e, en, a) in &enemies {
        if !en.is_dead() || paid.contains(&e) {
            continue;
        }
        paid.insert(e);
        if en.unrewarded {
            continue;
        }
        let (exp, sen) = reward(combat.npc(a));
        let sen = (sen as f32 * rate).round() as u32;
        earned.exp = earned.exp.wrapping_add(exp);
        earned.sen = earned.sen.wrapping_add(sen);
        let p = &mut config.player;
        p.sen = p.sen.saturating_add(sen);
        let points = gain(p, exp);
        info!("defeated {}: +{exp} experience, +{sen} Sen{}", combat.foe_of(a).chr, if points > 0 { format!(", +{points} skill point(s)") } else { String::new() });
    }
}

/// Death (no resurrection left): half the Sen and half the experience not yet a point are lost.
fn death_penalty(mut config: ResMut<crate::config::GameConfig>) {
    let p = &mut config.player;
    p.sen -= p.sen / 2;
    p.skill_exp -= p.skill_exp / 2;
    info!("death: {} Sen and {} experience left", p.sen, p.skill_exp);
}

pub struct ExperiencePlugin;

impl Plugin for ExperiencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Earned>().add_systems(Update, reward_kills);
        #[cfg(feature = "host")]
        app.add_systems(OnEnter(crate::menu::Screen::Dead), death_penalty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experience_fills_points_by_the_table() {
        let mut p = crate::config::PlayerConfig::default();
        p.skill_points = 0;
        assert_eq!(gain(&mut p, 499), 0);
        assert_eq!(gain(&mut p, 1), 1);
        assert_eq!((p.skill_points, p.skill_level, p.skill_exp), (1, 1, 0));
        // 514 + 528 for the next two, 10 over.
        assert_eq!(gain(&mut p, 514 + 528 + 10), 2);
        assert_eq!((p.skill_points, p.skill_exp), (3, 10));
        assert_eq!(to_next(25), 1093);
        assert_eq!(to_next(500), 83837);
    }

    #[test]
    fn an_ashina_soldier_pays_its_npc_param() {
        let npc = serde_json::json!({ "skillPoint": 21, "getSoul": 11 });
        assert_eq!(reward(&npc), (21, 11));
    }
}
