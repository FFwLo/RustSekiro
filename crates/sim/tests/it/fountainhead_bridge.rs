#[test]
fn arena_d_fountainhead_bridge() {
    let Some((arena, mut duel)) = super::arena_d::stage("fountainhead-bridge", 2_500_850, 2) else {
        return;
    };
    assert_eq!(
        arena.spec.story_time(&duel.spec),
        sekiro_sim::story::StoryTime::Night
    );
    // Phase two enables supporting phantoms; they must not replace the real Monk.
    duel.rules.fighters[1].vitality.deathblows_left = 2;
    duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
    assert_eq!(duel.events.as_ref().unwrap().boss_entity, 2_500_850);
    let events = duel.events.as_mut().unwrap();
    assert!(events.scripts.flag(12_505_952));
    for entity in [2_500_851, 2_500_853, 2_500_854] {
        assert_eq!(events.scripts.cast_scene.enabled.get(&entity), Some(&true));
    }
    events.scripts.set_flag(9309, true);
    assert!(events.standing_fog_walls().next().is_none());
}
