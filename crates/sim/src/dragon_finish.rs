//! Divine Dragon's TAE 932/104 special throw, at the posed eye dummy from ThrowParam.

use super::*;

impl Duel {
    pub fn dragon_eye(&self) -> Option<Placement> {
        if self.spec.chr != "c5200" {
            return None;
        }
        let row = self
            .throws
            .as_ref()?
            .table
            .rows
            .iter()
            .find(|r| r.id == 15_200_090)?;
        let model = self.enemy.bone_source_matrices();
        let &[point, forward, _] = self.fighters[1]
            .rig
            .frames(row.def_sorb_dummy, &model, self.enemy.source_to_world())
            .first()?;
        Some(Placement {
            position: point.to_array(),
            yaw: (-forward.x).atan2(-forward.z),
        })
    }

    pub(super) fn dragon_finishing_row(&self) -> Option<(i32, i32)> {
        if self.spec.chr != "c5200"
            || self.deathblow.is_some()
            || self.rules.fighters[0].dead()
            || !self.enemy.tae_frame().action_unlocks.contains(&104)
        {
            return None;
        }
        let row = self
            .throws
            .as_ref()?
            .table
            .rows
            .iter()
            .find(|r| r.id == 15_200_090)?;
        row.in_range(&placement(&self.player), &self.dragon_eye()?)
            .then_some((row.id, row.id))
    }

    pub(super) fn try_dragon_finish(&mut self) -> Option<(i32, i32)> {
        let rows = self.dragon_finishing_row()?;
        let row = self
            .throws
            .as_ref()?
            .table
            .rows
            .iter()
            .find(|r| r.id == rows.0)?
            .clone();
        let eye = self.dragon_eye()?;
        let owner = placement(&self.enemy);
        let delta = glam::Vec3::from_array(eye.position) - glam::Vec3::from_array(owner.position);
        let local = glam::Quat::from_rotation_y(-owner.yaw) * delta;
        let sorb = DummyFrame {
            position: local.to_array(),
            yaw: eye.yaw - owner.yaw,
        }
        .inverse();
        let times = crate::guardian_ape::finishing_times(&self.player.tae, &row)?;
        let duration = self.player.clip_duration(&row.attacker_anim())?;
        let (db, _) = Deathblow::begin_body(
            &row,
            times,
            sorb,
            duration,
            &placement(&self.player),
            &owner,
        );
        self.player
            .behavior
            .runtime
            .offsets
            .by_type
            .insert(THROW_ATK_OFFSET_TYPE, row.atk_anim_offset as u32);
        self.enemy
            .behavior
            .runtime
            .offsets
            .by_type
            .insert(THROW_DEF_OFFSET_TYPE, row.def_anim_offset as u32);
        self.player
            .behavior
            .runtime
            .fire_event(&format!("W_ThrowAtk{}", row.atk_anim));
        self.enemy
            .behavior
            .runtime
            .fire_event(&format!("W_ThrowDef{}", row.def_anim));
        self.last_throw_def_clip = row.defender_anim();
        self.deathblow = Some(db);
        self.defender_dying = 0;
        self.knockback = [None; 2];
        self.release_lock();
        Some(rows)
    }
}
