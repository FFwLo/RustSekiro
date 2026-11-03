export const meta = {
  name: 'sv1-bug-hunt-20',
  description: 'Twentieth bug-hunt round on Sv1: Wolf movement, grapple, stealth sight, hit resolve order, each finding adversarially verified',
  phases: [{ title: 'Find' }, { title: 'Verify' }],
}

// The Sv1 folder: pass its path as the workflow's args.root (a workflow script has no require / __dirname).
const ROOT = (typeof args === 'object' && args && args.root) || 'the shinobi-combat folder next to this repo'
const COMMON = `Project: Sv1 at ${ROOT}, a Rust/Bevy 0.19 recreation of Sekiro's combat built from the game's files (read docs/HANDOFF.md sections 4.19 to 4.38 for the earlier rounds' fixes so you do not report them again, and docs/kb/*.md for intended behaviour). READ-ONLY: do not edit any file, do not build, do not run sv1.exe. You may run "cargo test --release <name>" only if a test binary already exists and only for a quick check (no rebuild needed is not guaranteed; skip if it would rebuild).
Scope: shared engine and regular enemies only. Ignore anything specific to the 57 boss/mini-boss rows listed in extracted/enemies/npc_names.json (they belong to someone else) and ignore performance-only issues.
Report only REAL defects you can tie to exact code (file:line) with a concrete trigger: wrong behaviour, crash/panic, state that leaks across resets, wrong sign/units, a value contradicting the cited game data. No style, no TODO wishes, no "could be cleaner". Return at most 2 findings, the most important first; return an empty list if nothing solid.
Already known and being fixed (do not report): flag 24 for the player defender and its knockback, plunge fall check and per-target row, hurt_by_wolf only set on a reaction, respawn_spot offset for MSB enemies, area music stopping in Options, sandbox empty line-up respawn loop, menu hover overriding keys, burn spCategory stacking, tool dummies one frame old, enemy respawn crossfade and SnapPose, SkeletonRoot drawn pose feedback, CameraSnap focus height, IsTargetGuard / GetSp / GetSpRate ignoring the target, doping rates maxHpRate / maxStaminaRate / physicsAttackPowerRate not exported, resurrection HUD nodes, ai_hold not reading e.clash, firecracker SpEffects not added, enemy bullet shared hit list, DecalParam rows of npcs-exported enemies.`

const FINDING = {
  type: 'object',
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          title: { type: 'string' }, file: { type: 'string' }, line: { type: 'integer' },
          what: { type: 'string' }, evidence: { type: 'string' }, trigger: { type: 'string' },
          severity: { type: 'string', enum: ['crash', 'wrong-behaviour', 'minor'] }, fix_idea: { type: 'string' },
        },
        required: ['title', 'file', 'line', 'what', 'evidence', 'trigger', 'severity', 'fix_idea'],
      },
    },
  },
  required: ['findings'],
}
const VERDICT = {
  type: 'object',
  properties: { refuted: { type: 'boolean' }, in_scope: { type: 'boolean' }, confidence: { type: 'string', enum: ['low', 'medium', 'high'] }, reason: { type: 'string' } },
  required: ['refuted', 'in_scope', 'confidence', 'reason'],
}

const AREAS = [
  { key: 'movement', files: 'src/player.rs movement: walk / run / sprint, jumps, ledge hang and climb, falling and fall damage, crouch, slopes and steps against map.rs Terrain', focus: 'speeds or turn rates from the wrong param or units, fall damage thresholds wrong or applied twice, ledge grab states that can stick, crouch / stand transitions that leave the capsule wrong, movement while a reaction or menu should block it.' },
  { key: 'grapple', files: 'src/wire.rs and its use in player.rs and camera.rs', focus: 'wire targets chosen through walls or out of range, wire states that never end, velocity carried wrongly out of the wire, wire during reset or death, the aim marker and the chosen point disagreeing.' },
  { key: 'stealth-sight', files: 'src/stealth.rs and the enemy.rs targeting code (eye ranges, angles, hearing, alert levels), regular enemies only', focus: 'angles in the wrong units or measured from the wrong forward, ranges from the wrong NpcThinkParam field, crouch or grass not reducing sight as the data says, alert state not decaying or leaking across resets, sight through map geometry.' },
  { key: 'resolve-order', files: 'src/combat.rs resolve and the systems around it (hit queue, hit stop, deflect / guard decision, deathblow marks), with the FixedUpdate order in actor.rs', focus: 'two hits in one tick resolved in the wrong order, a hit applied after its attacker died or was reset, hit stop applied twice, a deflect decided from last tick state, the same attack hitting twice across anim restarts.' },
]

const results = await pipeline(
  AREAS,
  a => agent(`${COMMON}\n\nYour area: ${a.files}. Focus: ${a.focus}\nRead the code carefully, cross-check callers in other files, and report concrete defects.`, { label: `find:${a.key}`, phase: 'Find', schema: FINDING }),
  (r, a) => parallel((r?.findings ?? []).slice(0, 2).map((f, i) => () =>
    agent(`${COMMON}\n\nYou are a skeptic. Try hard to REFUTE this reported defect. Read the cited code and its callers. Refute it if the code does not do what is claimed, if the trigger cannot happen in play, if it is intended behaviour documented in docs/kb or a code comment, or if it only concerns boss rows. Default to refuted=true when uncertain.\n\nReport:\n${JSON.stringify(f, null, 1)}`, { label: `verify:${a.key}#${i}`, phase: 'Verify', schema: VERDICT })
      .then(v => ({ ...f, area: a.key, verdict: v }))))
)
const all = results.flat().filter(Boolean)
const confirmed = all.filter(f => f.verdict && !f.verdict.refuted && f.verdict.in_scope)
const refuted = all.filter(f => !(f.verdict && !f.verdict.refuted && f.verdict.in_scope))
log(`${confirmed.length} confirmed, ${refuted.length} refuted or out of scope`)
return { confirmed, refuted: refuted.map(f => ({ title: f.title, file: f.file, reason: f.verdict?.reason })) }