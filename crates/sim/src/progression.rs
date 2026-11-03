//! Wolf's Memory-based attack power, separate from Prayer Necklace growth.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackPower(u8);

impl Default for AttackPower {
    fn default() -> Self {
        Self(1)
    }
}

impl AttackPower {
    pub const CASTLE_ROOF: Self = Self(6);

    pub fn new(value: u8) -> Result<Self, String> {
        if (1..=99).contains(&value) {
            Ok(Self(value))
        } else {
            Err("attack power must be 1..=99".into())
        }
    }

    pub fn value(self) -> u8 {
        self.0
    }

    pub fn effect_id(self) -> i32 {
        160000 + i32::from(self.0)
    }

    /// Explicit attack power overrides the preset, regardless of argument order.
    pub fn from_options(value: Option<&str>, preset: Option<&str>) -> Result<Self, String> {
        let preset = match preset {
            None => Self::default(),
            Some("castle-roof") => Self::CASTLE_ROOF,
            Some(other) => return Err(format!("unknown progression preset {other}")),
        };
        value.map_or(Ok(preset), |v| {
            v.parse::<u8>()
                .map_err(|_| "attack power must be an integer in 1..=99".into())
                .and_then(Self::new)
        })
    }
}
