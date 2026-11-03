//! `npc-sim [chr] [steps]`: an NPC (default `c1010`) with the stand-in brain, approaching and
//! attacking a target standing 6 m away. Prints state and clip changes.

use sekiro_sim::ai::SimpleBrain;
use sekiro_sim::{Character, InputFrame};

fn main() {
    let mut args = std::env::args().skip(1);
    let chr = args.next().unwrap_or_else(|| "c1010".into());
    let steps: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(900);
    let mut npc = Character::load_npc(&sekiro_sim::cache::root(), &chr).expect("load npc");
    let dt = 1.0 / 60.0;
    let settled = npc.settle(dt, 300);
    println!(
        "{chr} settled after {settled} steps in {:?}",
        npc.behavior.runtime.main_state_path()
    );
    let target = [0.0, 0.0, -6.0];
    let mut brain = SimpleBrain::default();
    let mut last = String::new();
    for i in 0..steps {
        brain.think(&mut npc, target, dt);
        let r = npc.tick(&InputFrame::default(), dt);
        let key = format!(
            "{} {}",
            r.behavior.state_path.join("/"),
            r.animation.as_deref().unwrap_or("-")
        );
        if key != last || !r.behavior.script_errors.is_empty() {
            println!(
                "{i:5} action={:5} {:<50} {:<14} pos=({:+5.2},{:+5.2}) yaw={:+4.0} events={:?}{}",
                npc.npc.action,
                r.behavior.state_path.join("/"),
                r.animation.as_deref().unwrap_or("-"),
                r.position[0],
                r.position[2],
                r.yaw.to_degrees(),
                r.behavior.events,
                if r.behavior.script_errors.is_empty() {
                    String::new()
                } else {
                    format!(" ERRORS {:?}", r.behavior.script_errors)
                }
            );
            last = key;
        }
    }
}
