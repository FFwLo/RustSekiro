#[test]
fn hirata_father_stages_distinct_story_and_inner_identities() {
    for (name, entity, end) in [
        ("hirata-father", 1_000_900, 9317),
        ("hirata-inner-father", 1_000_990, 9346),
    ] {
        super::arenas_a_support::check(name, entity, Some((11_005_914, end)));
    }
}
