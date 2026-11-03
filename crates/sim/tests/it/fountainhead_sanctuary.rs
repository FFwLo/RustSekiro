#[test]
fn arena_d_fountainhead_sanctuary() {
    let Some((arena, mut duel)) = super::arena_d::stage("fountainhead-sanctuary", 2_500_810, 0)
    else {
        return;
    };
    assert_eq!(arena.layout.wolf_source, "MSB warp region 2502821");
    assert_eq!(
        arena.spec.story_time(&duel.spec),
        sekiro_sim::story::StoryTime::Afternoon
    );
    let cache = sekiro_sim::cache::root();
    let dragon = sekiro_sim::enemy::EnemySpec::from_cache(&cache, "c5200:52000000").unwrap();
    assert_eq!(
        arena.spec.story_time(&dragon),
        sekiro_sim::story::StoryTime::Morning
    );
    assert!(sekiro_sim::events::warp_point(&cache, 25, 0, 2_502_824).is_some());
    // Exercise the script's reviewed retry branch without pretending that one Duel can
    // yet count the aggregate HP and deaths of the entire Old Dragons cast.
    duel.events
        .as_mut()
        .unwrap()
        .scripts
        .set_flag(12_505_882, true);
    for _ in 0..420 {
        duel.enemy_active = false;
        duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
    }
    let events = duel.events.as_ref().unwrap();
    assert_eq!(events.boss_entity, 2_500_800);
    assert_eq!(events.boss_bar, Some(905200));
    assert_eq!(events.scripts.cast_scene.lighting, Some(1));
    assert_eq!(
        events.scripts.cast_scene.enabled.get(&8_907_090),
        Some(&true)
    );
    assert_eq!(
        events.scripts.cast_scene.backread_enabled.get(&2_505_800),
        Some(&false)
    );
    let point = sekiro_sim::events::warp_point(&cache, 25, 0, 2_502_824).unwrap();
    assert!((duel.player.body.position[0] - point.position[0]).abs() < 0.1);
    assert!((duel.player.body.position[2] - point.position[2]).abs() < 0.1);
}
