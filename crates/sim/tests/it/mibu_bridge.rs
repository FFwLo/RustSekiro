#[test]
fn arena_d_mibu_bridge() {
    let Some((arena, mut duel)) = super::arena_d::stage("mibu-bridge", 1_500_800, 1) else {
        return;
    };
    let events = duel.events.as_mut().unwrap();
    assert!(events.scripts.ended(11_505_810));
    assert!(!events.scripts.log.iter().any(|entry| matches!(
        entry,
        sekiro_sim::events::EventLog::Skipped {
            event: 11_505_810,
            bank: 2004,
            id: 34
        }
    )));
    events.scripts.set_flag(9306, true);
    assert!(events.standing_fog_walls().next().is_none());
    assert_eq!(
        arena.spec.story_time(&duel.spec),
        sekiro_sim::story::StoryTime::Morning
    );
}
