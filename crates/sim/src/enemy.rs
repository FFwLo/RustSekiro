//! Which enemy a duel loads: its character model, NpcParam and NpcThinkParam rows and the map
//! script bundle its Lua AI lives in.
//!
//! The rows are the ones the map layouts place: the soldier as in `m11_00_00_00`, the Great
//! Shinobi Owl as part `c5060_0000` of `m11_01_00_00` (the Ashina Castle roof). See
//! `docs/OWL.md`.

/// One enemy as the duel loads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnemySpec {
    /// Character id, such as `c1010`: model, animations, HKS script and behaviour graph.
    pub chr: String,
    /// NpcParam row: HP, posture, deathblow marks, hit radius, behaviour variation.
    pub npc_param: i32,
    /// NpcThinkParam row: the logic and battle goal of its Lua AI.
    pub think: i32,
    /// The unpacked map script bundle that holds its `<id>_logic.lua` and `<id>_battle.lua`.
    pub ai_bundle: String,
    /// The AI starts in the battle target state instead of noticing Wolf first. Boss fights
    /// begin after a cutscene with the boss already fighting; NpcParam 50600000 also carries
    /// the find/battle state SpEffect 200004 as a resident effect (that this is why is
    /// inferred, see `docs/OWL.md`).
    pub starts_in_battle: bool,
}

impl EnemySpec {
    /// The Ashina soldier with a one-handed sword (NpcParam and NpcThinkParam 10100000).
    pub fn soldier() -> Self {
        Self::from_cache(&Self::default_cache(), "c1010:10100000")
            .expect("soldier placement is extracted")
    }

    /// The Great Shinobi Owl on the Ashina Castle roof:
    /// NpcParam 50600000, NpcThinkParam 50601000 (logic 506000, battle goal 506001), as part
    /// `c5060_0000` of `m11_01_00_00` places him.
    pub fn owl() -> Self {
        Self::from_cache(&Self::default_cache(), "c5060:50600000")
            .expect("Owl placement is extracted")
    }

    /// A placed character id, optional NpcParam row, or the soldier/Owl compatibility aliases.
    pub fn by_name(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "c1010" | "soldier" => Ok(Self::soldier()),
            "c5060" | "owl" => Ok(Self::owl()),
            "inner-isshin" => Self::from_cache(&Self::default_cache(), "inner-isshin"),
            other => crate::registry::Registry::load(&Self::default_cache())?.resolve(other),
        }
    }

    /// Resolve a selection using the caller's cache rather than the repository default.
    pub fn from_cache(cache: &std::path::Path, name: &str) -> Result<Self, String> {
        let name = name.to_ascii_lowercase();
        if name == "inner-isshin" {
            return crate::registry::Registry::load(cache)?
                .entity("m11_02_00_00", 1120890)
                .map(|(spec, _)| spec);
        }
        let selection = match name.as_str() {
            "soldier" => "c1010:10100000",
            "owl" => "c5060:50600000",
            _ => &name,
        };
        crate::registry::Registry::load(cache)?.resolve(selection)
    }

    /// The enemy's name for the HUD.
    pub fn display_name(&self) -> &'static str {
        static ROSTER: std::sync::OnceLock<Option<std::sync::Arc<crate::registry::Registry>>> =
            std::sync::OnceLock::new();
        if let Some(name) = ROSTER
            .get_or_init(|| crate::registry::Registry::load(&Self::default_cache()).ok())
            .as_ref()
            .and_then(|r| {
                if self.chr == "c5400" && matches!(self.think, 54030000 | 54030100) {
                    r.boss_name(905403)
                } else if crate::owl_father::owns(self) && self.think == 50630000 {
                    r.boss_name(905063)
                } else if crate::owl_father::owns(self) {
                    r.boss_name(905061)
                } else {
                    r.display_name(&self.chr, self.npc_param)
                }
            })
        {
            return name;
        }
        match self.chr.as_str() {
            "c1010" => "Ashina Soldier",
            "c5060" => "Great Shinobi Owl",
            _ => "Enemy",
        }
    }

    fn default_cache() -> std::path::PathBuf {
        crate::cache::root()
    }

    /// The model number ThrowParam's `defChrId` uses (`c1010` -> 1010).
    pub fn throw_chr(&self) -> i32 {
        self.chr.trim_start_matches('c').parse().unwrap_or(0)
    }
}

impl Default for EnemySpec {
    fn default() -> Self {
        Self::soldier()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve() {
        assert_eq!(EnemySpec::by_name("owl").unwrap().chr, "c5060");
        assert_eq!(EnemySpec::by_name("C1010").unwrap(), EnemySpec::soldier());
        assert_eq!(EnemySpec::by_name("c5070").unwrap().chr, "c5070");
        assert_eq!(EnemySpec::owl().throw_chr(), 5060);
    }
}
