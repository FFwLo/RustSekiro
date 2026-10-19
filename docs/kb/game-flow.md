# Sekiro game flow

This file walks the whole game from the first controllable moment to the end of each ending. It is built from the flow records in `docs/kb/atlas/parts/flow.json`; the ids there match the step numbers here (step 12 is `flow.12-...`, a branch is the step plus a letter).

Sources, in order: the extracted event scripts and params, the DarkScript3 instruction names, our notes, the existing atlas, and the web (Fextralife and GameSpot, summarized). The order of the run follows the all-bosses recording listed in `docs/kb/clips.md`.

Gate means what has to be true before the step. Opens means the later steps that name this step as a requirement.

Route in one line: Reservoir prologue, lost duel, Dilapidated Temple, Outskirts, Hirata memory, Ashina Castle, then four regions in a free order (Senpou, Sunken Valley, Ashina Depths, Mibu) that feed the castle invasion, Fountainhead and the Reservoir finale. Shura ends the game early at the castle; the other three endings share Fountainhead, the Divine Dragon and the Sword Saint.

## Walkthrough

### Step 01. New game: Ashina Reservoir opening

A new save starts in the Ashina Reservoir (m11_02) after the opening movie. Common event 50 raises the prologue flag 8306 while 11120100 is off, then event 11120100 plays cutscene 11020020, sets the first respawn point 1122102 and saves.

Gate: none beyond the start of the game. Opens: Kuro's Charm (optional start item); Reservoir pickup: Ornamental Letter; Reservoir: the Kusabimaru scene.

Confidence: data.

### Step 01b. Kuro's Charm (optional start item)

Kuro's Charm (goods 9003) comes from lot 3300 through m11_02 event 11120120 after the opening scene (it checks flag 6910), and again as NPC lots 61006 and 3300. Giving it back is what turns on Charmless mode later.

Gate: New game: Ashina Reservoir opening. Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 02. Reservoir pickup: Ornamental Letter

On the walk through the Reservoir the player can pick up the Ornamental Letter (goods 9020), a plain item lot (1120000) in m11_02. No gate needs it.

Gate: New game: Ashina Reservoir opening. Opens: nothing further (optional or a dead end).

Note: Community sources place it in a prologue well; the lot only proves it is in m11_02.

Confidence: data.

### Step 03. Reservoir: the Kusabimaru scene

Walking into area 1122371 while the prologue flag 8306 is on triggers cutscene 11020070. Wolf then gets the Kusabimaru (lot 3001, goods 2300), the Wolf outfit (lot 3080, armor 131000) and respawn 1122200.

Gate: New game: Ashina Reservoir opening. Opens: Reservoir: the gate and the Yamauchi leader.

Confidence: data.

### Step 04. Reservoir: the gate and the Yamauchi leader

An object prompt at the gate (object 1121711) asks a yes or no question. Yes plays cutscene 11020090 and moves the player to 1122202; the all-bosses run counts the Yamauchi leader fight as boss 1. The scene resets flags 1020-1034 and sets 11120392.

Gate: Reservoir: the Kusabimaru scene. Opens: Prologue: the lost duel with Genichiro.

Note: The Yamauchi leader has no named boss bar in the dumped events; the name comes from the recording chapter list.

Confidence: data.

### Step 05. Prologue: the lost duel with Genichiro

In the silvergrass field Genichiro's bar (907101) is shown after cutscene 11020010. The fight cannot be won: when the HP check trips, event 11125800 sets 9300, plays 11020001 (or 11020000) and warps Wolf to 1102621 at the Dilapidated Temple. It is the first boss flag and starts the temple wake-up.

Gate: Reservoir: the gate and the Yamauchi leader. Opens: Dilapidated Temple: waking up; Resurrection arrives.

Confidence: data.

### Step 06. Dilapidated Temple: waking up

After the defeat scenes event 11100100 (m11_00) runs: cutscene 11000000, lot 3070 (armor 102000), lot 3060 (the Shinobi Prosthetic), idol flag 11100000, and flag 8306 off so the prologue state ends. The Sculptor is here and the temple idol (BonfireWarpParam 100 and 1100, home) is the first save point.

Gate: Prologue: the lost duel with Genichiro. Opens: The Shinobi Prosthetic arrives.

Confidence: data.

### Step 07. The Shinobi Prosthetic arrives

Lot 3060 puts the Shinobi Prosthetic (goods 2310, flag 50003060) on Wolf in the temple wake-up. The first tool, the emblem stock and the skill pages open from here, and the Sculptor sells upgrades. The grapple, shuriken and firecracker routes of every later area need it.

Gate: Dilapidated Temple: waking up. Opens: Ashina Outskirts: first steps.

Confidence: data.

### Step 08. Resurrection arrives

Common event 250 runs once after the Genichiro loss (9300 on): it waits for one of flags 131-143 and for Wolf's event message 50, then sets 9207, awards lot 3010 (Resurrection, goods 2000), plays the common cutscene 89000099 and adds a resurrection node. It sets 250, 6250 and 8250 and shows the tutorial text.

Gate: Prologue: the lost duel with Genichiro. Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 09. The temple hub follows the story clock

The temple and the Outskirts change light and sky with the cumulative flags 9800-9803. Common event 820 raises 9800 after one of the four time bosses (9303-9306) is down and 9801 after three; event 830 mirrors them into 9820-9823; event 100 picks the sky dome group (m89 parts 8907000-8907100). After the castle fight the Outskirts goes from morning to day.

Gate: The four time bosses and the clock. Opens: Offering the Charm: warp to Hirata; The four time bosses and the clock; When the player can return to the temple hub.

Note: 9802 and 9803 are not set by any EMEVD instruction in the dumped files; talk scripts or other state set them (open question).

Confidence: data.

### Step 10. Ashina Outskirts: first steps

The temple opens to the Outskirts (m11_00). Event 11100120 shows the area welcome message (1100) and sets 8201 and 9110. Idols along the road are Ashina Outskirts, Outskirts Wall Gate Path, Stairway, Underbridge Valley, Ashina Castle Gate Fortress, Ashina Castle Gate and, later, Flames of Hatred.

Gate: The Shinobi Prosthetic arrives. Opens: Outskirts mini-bosses (optional); Gyoubu Masataka Oniwa.

Confidence: data.

### Step 10b. Outskirts mini-bosses (optional)

On the road the all-bosses run meets General Naomori Kawarada, the first Chained Ogre, General Tenzen Yamauchi, Shinobi Hunter Enshin of Misen and a first Headless. They use mini-boss bars (2003[87]), are not needed to open the castle, and pay sen, XP and some Prayer Beads.

Gate: Ashina Outskirts: first steps. Opens: nothing further (optional or a dead end).

Confidence: recorded.

### Step 11. Gyoubu Masataka Oniwa

Entering area 1102810 starts event 11105810 (camera set 500, bar 905081); defeat sets 9301 and 6801 through 11105800. Event 300 then pays lot 2000 (goods 2910) and lot 2001 (Memory: Gyoubu Oniwa). The way to the Castle Gate opens.

Gate: Ashina Outskirts: first steps. Opens: Gate: Young Lord's Bell Charm; Road to the Castle Gate; Required bosses by ending.

Confidence: data.

### Step 12. Gate: Young Lord's Bell Charm

The Young Lord's Bell Charm (goods 9010, lots 51600 and 61600, flag 50006160) is the key to the Hirata Estate memory. No EMEVD award gives it, so a talk gives it. Offering it at the temple Buddha starts the memory.

Gate: Gyoubu Masataka Oniwa. Opens: Offering the Charm: warp to Hirata; Key items that gate progress.

Note: The exact giver and its flag gate sit in an ESD (talk) file, not in the event scripts. Open question: whether Gyoubu is needed first.

Confidence: community.

### Step 13. Offering the Charm: warp to Hirata

Event 11100628 in m11_00 plays one of the entry cutscenes 11000030-11000036 (picked by 9800-9802 and 8302) and warps Wolf to the Hirata Estate, point 1002630 in m10_00. Event 11100629 warps to 1000631 for the other visit.

Gate: Gate: Young Lord's Bell Charm; The temple hub follows the story clock. Opens: Hirata optional rooms; Lady Butterfly; Second Hirata memory: Owl (Father).

Confidence: data.

### Step 13b. Hirata optional rooms

Hirata Estate has six idols: Dragonspring, Estate Path, Bamboo Thicket Slope, Main Hall, Hidden Temple and Audience Chamber (BonfireWarpParam 1000-1005). The Hidden Temple Key (goods 9403, lot 61200, flag 50006120) opens the Hidden Temple. Juzou the Drunkard, Enshin and Masanaga are optional mini-bosses.

Gate: Offering the Charm: warp to Hirata. Opens: nothing further (optional or a dead end).

Confidence: community.

### Step 14. Lady Butterfly

Crossing area 1002810 plays cutscene 10000010 (warp 1002815) and shows bar 905090 (event 11005810). Defeat sets 9302 and 6802 (event 11005800). Event 300 pays lot 2030 (Sakura Droplet, goods 9009) and lot 2031 (Memory: Lady Butterfly).

Gate: Offering the Charm: warp to Hirata. Opens: Hirata burns and Wolf returns; Required bosses by ending.

Confidence: data.

### Step 15. Hirata burns and Wolf returns

Right after Butterfly dies event 11005800 sets respawn 1102631 and flags 9261, 9402 and 11000805, then plays cutscene 10000000 (one of five lighting variants chosen by 9800-9803) and warps Wolf to the temple. The estate is shown burning and the first memory is over.

Gate: Lady Butterfly. Opens: Second Hirata memory: Owl (Father); World-state change table.

Confidence: data.

### Step 15b. Second Hirata memory: Owl (Father)

The Father's Bell Charm (goods 9011, lot 62302, flag 50006232) starts a second visit with the estate burning (stage 8304). Event 11105631 disables three Hirata idols; Owl (Father) (bar 905061, event 11005910) is fought for Memory: Foster Father and the Aromatic Flower (lot 2100, goods 2503) the Purification ending needs.

Gate: Hirata burns and Wolf returns; Offering the Charm: warp to Hirata. Opens: Optional bosses.

Note: Who gives the Father's Bell Charm and when sits in a talk file (open question).

Confidence: data.

### Step 16. Road to the Castle Gate

From the Gate Fortress the road runs through the Castle Gate idol into Ashina Castle (m11_01). No dumped flag blocks the castle road: the Butterfly flag 9302 is read only inside Hirata, and the Genichiro fight gates on its own area and the time flags.

Gate: Gyoubu Masataka Oniwa. Opens: Ashina Castle: the climb.

Note: Community says Hirata comes first; the events show Hirata changes the world but do not show a hard door.

Confidence: data.

### Step 17. Ashina Castle: the climb

The castle (m11_01) is climbed through the Ashina Castle, Upper Tower Antechamber, Ashina Dojo, Castle Tower Lookout and Kuro's Room idols. Event 11110717 plays an ambient cutscene 11010100 and 11115740 plays 11010110. Mini-bosses on the way are Blazing Bull, a Lone Shadow, Shikibu Yamauchi, Matsumoto and Jinsuke Saze.

Gate: Road to the Castle Gate. Opens: Ashina Elite and Lone Shadow fights (optional); Genichiro at the castle top.

Confidence: data.

### Step 17b. Ashina Elite and Lone Shadow fights (optional)

Several mini-bosses are side fights: Ashina Elite Jinsuke Saze in the castle, Ashina Elite Ujinari Mizuo after the Divine Dragon, Lone Shadow Longswordsman, Masanaga (twice) and Vilehand, and the Seven Ashina Spears. They pay sen and Prayer Beads and gate nothing.

Gate: Ashina Castle: the climb. Opens: nothing further (optional or a dead end).

Confidence: recorded.

### Step 18. Genichiro at the castle top

Entering area 1112811 (event 11115810) plays 11010010 or 11010011 (picked by the time flags), warps Wolf to 1112812 and shows bar 907100. Event 11115800 ends at once when 8301 is on, so the fight is only there before the invasion.

Gate: Ashina Castle: the climb. Opens: Genichiro: Way of Tomoe.

Confidence: data.

### Step 19. Genichiro: Way of Tomoe

At zero bars on 1110800 event 11115820 sets 11115803 and plays cutscene 11010020, then swaps to entity 1110801 with bar 907110 (Genichiro, Way of Tomoe). Defeating that bar runs 11115800: achievement 22, flags 9303 and 6803, lot 2080 (Bloodsmoke Ninjutsu, goods 2100) and lot 2081 (Memory: Genichiro).

Gate: Genichiro at the castle top. Opens: The castle after Genichiro; Required bosses by ending.

Confidence: data.

### Step 20. The castle after Genichiro

Defeat sets 9303 and 8305 (castle stage on), then picks cutscene 11010030 or 11010031 by how many of 9304-9306 are down; the castle sky parts 8907xxx change. Kuro and Emma speak, Isshin's wish is explained and the next tasks (Gun Fort Shrine Key, Mortal Blade) are given.

Gate: Genichiro: Way of Tomoe. Opens: Gate: Gun Fort Shrine Key; Abandoned Dungeon (via the castle).

Confidence: data.

### Step 21. Gate: Gun Fort Shrine Key

The Gun Fort Shrine Key (goods 9405, lot 1110860, flag 51110860) is a treasure in Ashina Castle. It opens the shrine on the way to Sunken Valley (Under-Shrine Valley idol 1700).

Gate: The castle after Genichiro. Opens: Sunken Valley and Gun Fort; Key items that gate progress.

Note: Fextralife says Kuro hands the key over after Genichiro; the lot is a map treasure in m11_01.

Confidence: data.

### Step 22. Abandoned Dungeon (via the castle)

Past the Kuro's Room idol the castle opens to the Abandoned Dungeon (m13). The Abandoned Dungeon Entrance idol (BonfireWarpParam 1117, entity 1111955, flag 11110005) is the last castle idol before it. The dungeon holds the lift up to Senpou Temple and the Underground Waterway idols (1300, 1301).

Gate: The castle after Genichiro. Opens: Senpou Temple, Mt. Kongo.

Confidence: community.

### Step 23. Senpou Temple, Mt. Kongo

A lift in the Abandoned Dungeon (m13) leads to Senpou Temple (m20). Idols: Senpou Shugendo 2001950, Temple Grounds, Main Hall, Inner Sanctum 2001954, Sunken Valley Cavern and Bell Demon's Temple (BonfireWarpParam 2000 to 2006). The Illusive Hall mechanism sits here.

Gate: Abandoned Dungeon (via the castle). Opens: Illusive Hall Bell (optional branch); Bell Demon (optional branch); Senpou mini-bosses (optional); Folding Screen Monkeys.

Confidence: data.

### Step 23b. Illusive Hall Bell (optional branch)

Event 12005210 lets the player ring the hall bell: action button on object 2001200, cutscene 20000000 (warp 2002200), lot 2000060 gives the Illusive Hall Bell (goods 3990). It reopens the Monkeys' arena later.

Gate: Senpou Temple, Mt. Kongo. Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 23c. Bell Demon (optional branch)

At the Bell Demon's Temple the player can defeat the Iron Bell. Event 12005200 sets 9830 and lot 3400 gives the Bell Demon (goods 3730, a plain key-style item).

Gate: Senpou Temple, Mt. Kongo. Opens: nothing further (optional or a dead end).

Note: Goods row 3730 is type 0 (a normal item); what it does in the game is not decoded here.

Confidence: data.

### Step 23d. Senpou mini-bosses (optional)

Senpou has a few optional mini-bosses on the way up (the recording counts the Armored Warrior among them). They pay sen and XP and are not gates.

Gate: Senpou Temple, Mt. Kongo. Opens: nothing further (optional or a dead end).

Note: Names and counts come from the recording only.

Confidence: recorded.

### Step 24. Folding Screen Monkeys

Entering the Inner Sanctum arena starts event 12005810; defeat sets 9305 and 9405 through 12005800. Event 300 pays lot 2020 (Puppeteer Ninjutsu, goods 2110) and lot 2021 (Memory: Screen Monkeys, goods 5203). The inner sanctum scene (event 12005906) follows.

Gate: Senpou Temple, Mt. Kongo. Opens: Inner Sanctum: the Divine Child.

Confidence: data.

### Step 25. Inner Sanctum: the Divine Child

After the Monkeys the Divine Child of Rejuvenation waits in the Inner Sanctum. Event 12000700 plays cutscene 20000020 on talk flag 72000002 and lot 3090 gives armor 101000. The Mortal Blade is given here later, as is the Return questline.

Gate: Folding Screen Monkeys. Opens: Return questline items; Gate: the Mortal Blade.

Confidence: data.

### Step 25b. Return questline items

The Return ending needs items from several NPC talks: Rice for Kuro (goods 9090, lot 62210), Frozen Tears (goods 9091, lot 62212), Holy Chapter: Infested (9215 lot 63200, 9228 lot 2000820), Holy Chapter: Dragon's Return (9209, lot 2000010, event 12005218), Pinwheels (9080 lot 2000030, 9081 lot 2000170). Network flags 1040-1054 are tracked by common_func event 20006111.

Gate: Inner Sanctum: the Divine Child. Opens: Ending: Return.

Note: The step order inside the quest comes from community text; only the item ids and lots are from the data.

Confidence: data.

### Step 26. Gate: the Mortal Blade

The Mortal Blade (goods 2400) comes as lot 3020 (and 62200, flag 50006224). Mortal Draw is lot 62202 and Empowered Mortal Draw lot 62204. It is the first of three castle-invasion key items; the other two are the Lotus (2500) and the Shelter Stone (2501).

Gate: Inner Sanctum: the Divine Child. Opens: Castle invasion (stage 8301); Key items that gate progress.

Confidence: data.

### Step 27. Sunken Valley and Gun Fort

The Gun Fort Shrine Key (goods 9405) opens the way to the Sunken Valley (m17). Idols: Under-Shrine Valley 1701957, Sunken Valley, Gun Fort, Riven Cave, Bodhisattva Valley 1701958 and Ape's Watering Hole (BonfireWarpParam 1700 to 1705).

Gate: Gate: Gun Fort Shrine Key. Opens: Serpent Viscera (optional branch); Guardian Ape; Bodhisattva Valley (optional branch); Ashina Depths.

Confidence: data.

### Step 27b. Serpent Viscera (optional branch)

Fresh Serpent Viscera (goods 9192, lot 50100000, event 11705614) is dropped when the Great Serpent (entity 1700600) is dead (flag 9351). Dried Serpent Viscera is lot 1700810 (goods 9193). Both are Dragonrot cure ingredients.

Gate: Sunken Valley and Gun Fort. Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 28. Guardian Ape

In the Ape's Watering Hole event 11705810 starts the fight; defeat sets 9304 through 11705800 and event 300 pays lot 2010 (Slender Finger, goods 9790) and lot 2011 (Memory: Guardian Ape, goods 5204). Event 912 then sets 8303 so the Headless Ape appears later.

Gate: Sunken Valley and Gun Fort; The four time bosses and the clock. Opens: Gate: Lotus of the Palace; Headless Ape (stage 8303).

Confidence: data.

### Step 29. Gate: Lotus of the Palace

Behind the Guardian Ape lies the Lotus of the Palace (goods 2500, lot 1700000, event 11705200). It is the second castle-invasion key item.

Gate: Guardian Ape. Opens: Castle invasion (stage 8301); Key items that gate progress.

Confidence: data.

### Step 29b. Bodhisattva Valley (optional branch)

A side path leads to Bodhisattva Valley (idol 1701958) with optional foes and treasures. It is not a gate.

Gate: Sunken Valley and Gun Fort. Opens: nothing further (optional or a dead end).

Confidence: community.

### Step 30. Ashina Depths

From the Sunken Valley the path leads past the Great Serpent and a steep drop to the Ashina Depths (m17). Idols: Ashina Depths 1701955, Poison Pool 1701954 and Guardian Ape's Burrow 1701956 (BonfireWarpParam 1500 to 1502). The Bottomless Hole (1301) is another entrance.

Gate: Sunken Valley and Gun Fort. Opens: Ashina Depths mini-bosses (optional); Hidden Forest and Mibu Village.

Confidence: data.

### Step 30b. Headless Ape (stage 8303)

After the Guardian Ape (9304) and before the optional Headless is killed (9314), stage 8303 is on and the Headless Ape appears in the Burrow (event 11705850, entity 1700850). Defeat sets 9307 and 9314; lots 2015 and 2016 pay a Prayer Bead each (goods 4000) and lot 2017 the memory (goods 5213). Idol 1701956 is disabled by 8307 (event 920) when no 79000/79100 item is held.

Gate: Guardian Ape. Opens: Optional bosses; World-state change table.

Confidence: data.

### Step 30c. Ashina Depths mini-bosses (optional)

The Depths and the Poison Pool hold several mini-bosses (the recording lists several in the Depths and the Poison Pool). They pay XP and sen and are not gates.

Gate: Ashina Depths. Opens: nothing further (optional or a dead end).

Note: Some names in this record are recording based and not all verified.

Confidence: recorded.

### Step 31. Hidden Forest and Mibu Village

The Depths lead up through the Hidden Forest into Mibu Village (m15). Idols: Hidden Forest 1501950, Mibu Village 1501951, Water Mill 1501952 and Wedding Cave Door 1501953 (BonfireWarpParam 1503 to 1506). The village is where the Dragonrot story shows up.

Gate: Ashina Depths. Opens: Corrupted Monk (Mibu illusion); Gate: Mibu Breathing Technique; Gate: Shelter Stone.

Confidence: data.

### Step 32. Corrupted Monk (Mibu illusion)

In the Water Mill area event 11505810 starts the fight, with the monk's arena set by the illusion. Defeat sets 9306 through 11505800 and event 300 pays lot 2051 (Memory: Corrupted Monk, goods 5205); lot 2050 has no row.

Gate: Hidden Forest and Mibu Village; The four time bosses and the clock. Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 33. Gate: Mibu Breathing Technique

The Mibu Breathing Technique (goods 2420, type 7, a skill-style row) has no item lot, so a talk gives it. The existing atlas notes that Wolf can dive only after learning it. It is not a castle gate by itself.

Gate: Hidden Forest and Mibu Village. Opens: nothing further (optional or a dead end).

Confidence: community.

### Step 34. Gate: Shelter Stone

The Shelter Stone (goods 2501) is in the Wedding Cave at lot 1500050 (event 11505290), and also NPC lot 3030. It is the third castle-invasion key item.

Gate: Hidden Forest and Mibu Village. Opens: Castle invasion (stage 8301); Key items that gate progress.

Confidence: data.

### Step 35. The four time bosses and the clock

Genichiro (9303), Guardian Ape (9304), Folding Screen Monkeys (9305) and Corrupted Monk (9306) each advance the clock. Event 820 raises 9800 after one and 9801 after three; event 901 records which was first in 8405-8408. Event 830 mirrors them to 9820-9823.

Gate: The temple hub follows the story clock. Opens: The temple hub follows the story clock; Guardian Ape; Corrupted Monk (Mibu illusion).

Confidence: data.

### Step 36. Castle invasion (stage 8301)

Common event 910 sets 8301 once the player holds Mortal Blade (2400), Lotus (2500) and Shelter Stone (2501). It disables 9 castle and reservoir idols, sets flag 11110005 and respawn 1112955. Event 11115720 plays cutscene 11010040 (warp 1112722) on entering area 1112811.

Gate: Gate: the Mortal Blade; Gate: Lotus of the Palace; Gate: Shelter Stone. Opens: Idol changes during the invasion; Invaded castle: the ascent; Great Shinobi Owl; World-state change table; The four regions can be done in a free order.

Confidence: data.

### Step 36b. Idol changes during the invasion

Stage 8301 disables nine castle and reservoir idols and event 11115200 re-registers the Castle Gate idol (flag 11110000, entity 1111950) when the fight resolves. Stage 8302 later disables castle idols 1111950/51/52/54/56/57 and Outskirts idols 1101951-56, plus 1121950/51.

Gate: Castle invasion (stage 8301); Back at the castle (stage 8302). Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 37. Invaded castle: the ascent

With 8301 on the castle fills with enemies and the way up runs through the Tower and the Great Shinobi Owl's roofs. Idols appear and vanish with the stage. The Tower Gate idols mark the Owl's approach.

Gate: Castle invasion (stage 8301). Opens: nothing further (optional or a dead end).

Confidence: community.

### Step 38. Great Shinobi Owl

Entity 905060's bar starts the fight; defeat sets 9308 (events 11115860/50/70) and event 300 pays lot 2090 (Aromatic Branch, goods 2502) and lot 2091 (Memory: Great Shinobi, goods 5206). Stage 8302 unloads it. Its father, the Owl Father (9317), is a separate fight in the second Hirata memory (stage 8304).

Gate: Castle invasion (stage 8301). Opens: The Iron Code choice; Required bosses by ending.

Confidence: data.

### Step 39. The Iron Code choice

After the Owl (9308), the story branches: Kuro and the Dragon path (Fountainhead) or Emma's Iron Code path (Shura). The choice is made in talks that set the branch flags.

Gate: Great Shinobi Owl. Opens: Shura: Emma, the Gentle Blade; Kuro's road: the Wedding Cave Door.

Note: Exact ESD flag for the choice is not decoded here. Flag 8415, set by a talk, is read by the Wedding Cave warp (step 43).

Confidence: community.

### Step 40. Shura: Emma, the Gentle Blade

Event 11115900 starts Emma with bar 907400 and cutscene 11010060 (warp 1112812). Defeat sets 9315. Emma's fight is the first half of the Shura route.

Gate: The Iron Code choice. Opens: Shura: Isshin Ashina; Required bosses by ending.

Confidence: data.

### Step 41. Shura: Isshin Ashina

After Emma the Isshin Ashina fight (bar 905401, events 11115920/30) sets 9316. Event 300 pays lot 2120 (One Mind, goods 2462) and lot 2121 (Memory: Isshin Ashina, goods 5212).

Gate: Shura: Emma, the Gentle Blade. Opens: Ending: Shura; Required bosses by ending.

Confidence: data.

### Step 42. Ending: Shura

With 9316 on, event 11110100 plays cutscene 11010070 and the credits follow. Ending flag 6833 and achievement 12; the award lot 54300000 has no row in ItemLotParam.

Gate: Shura: Isshin Ashina. Opens: The four endings in one table.

Confidence: data.

### Step 43. Kuro's road: the Wedding Cave Door

On the non-Shura path, the Wedding Cave Door idol (1501953) opens to a palanquin ride to Fountainhead. Event 11505695 (m15) plays cutscene 15000000 and warps to m25 point 2502822, gated by flag 8415 set in a talk.

Gate: The Iron Code choice. Opens: Fountainhead Palace.

Confidence: data.

### Step 44. Fountainhead Palace

Fountainhead (m25) opens with the idols at 2500 to 2508. It is the final region before the Dragon and holds the Water of the Palace (goods 9070, lot 2500650).

Gate: Kuro's road: the Wedding Cave Door. Opens: Fountainhead: the route to the monk; Fountainhead mini-bosses (optional).

Confidence: data.

### Step 45. Fountainhead: the route to the monk

The palace is a long climb with water areas and many elites. A Great White Whisker (goods 9071, lot 50500000, event 11705535) is a Sunken Valley item that feeds a side quest.

Gate: Fountainhead Palace. Opens: nothing further (optional or a dead end).

Confidence: community.

### Step 46. True Corrupted Monk

The Fountainhead monk (bar 2500850, events 12505960/50) sets 9309 and pays lot 2060 (Dragon's Tally Board, goods 9060) and lot 2061 (Memory: True Monk, goods 5208).

Gate: none beyond the start of the game. Opens: Required bosses by ending.

Confidence: data.

### Step 46b. Fountainhead mini-bosses (optional)

Okami Leader Shizu (entity 2500406) and a few other mini-bosses guard the climb. They are not gates.

Gate: Fountainhead Palace. Opens: nothing further (optional or a dead end).

Confidence: recorded.

### Step 48. Old Dragons of the Tree and the Divine Dragon

Event 12505801 runs the Old Dragons (bar 905300, cutscene 25000000 warp 2502821). Then event 12505885 plays cutscene 25000020 (warp 2502824) and 12505880 starts the Divine Dragon (bar 905200); defeat sets 9310 and 9410, warps to m11_01 point 1110799 and sets respawn 1112953.

Gate: none beyond the start of the game. Opens: Gate: Divine Dragon's Tears; Back at the castle (stage 8302); Required bosses by ending.

Confidence: data.

### Step 49. Gate: Divine Dragon's Tears

Divine Dragon's Tears (goods 9000, lot 2070) are paid at the Dragon's death. Offering them is the base of three endings.

Gate: Old Dragons of the Tree and the Divine Dragon. Opens: Key items that gate progress.

Confidence: data.

### Step 50. Back at the castle (stage 8302)

Common event 911 sets 8302 when 9310 is on. The castle reloads at night, six castle idols and the Outskirts idols are disabled, and the Hirata memories change. Kuro's talk opens the Reservoir path.

Gate: Old Dragons of the Tree and the Divine Dragon. Opens: Idol changes during the invasion; Demon of Hatred (optional, stage 8302); Gate: Secret Passage Key; World-state change table.

Confidence: data.

### Step 50b. Demon of Hatred (optional, stage 8302)

Events 11105900/11105910 run the Demon of Hatred (bar 907021) only when 8302 is on, in the Outskirts Flames of Hatred area. Defeat sets 9313; lot 2040 pays two Lapis Lazuli (goods 6400) and lot 2041 the memory (goods 5210).

Gate: Back at the castle (stage 8302). Opens: Optional bosses.

Confidence: data.

### Step 50c. Enemy swaps by world stage

Stages 8301, 8302, 8303 and 8304 swap enemy rosters: the castle becomes invaded, the Outskirts and Hirata go to the night versions, and the Burrow gets the Headless Ape.

Gate: none beyond the start of the game. Opens: nothing further (optional or a dead end).

Confidence: data.

### Step 52. Gate: Secret Passage Key

The Secret Passage Key (goods 9404, lot 62306) opens the passage to the Reservoir finale.

Gate: Back at the castle (stage 8302). Opens: Return to the Reservoir; Key items that gate progress.

Confidence: data.

### Step 53. Return to the Reservoir

Through the Secret Passage the player enters the Ashina Reservoir (idols 1118 and 1119). It is now in its final stage.

Gate: Gate: Secret Passage Key. Opens: Genichiro, Way of Tomoe (Reservoir).

Confidence: data.

### Step 54. Genichiro, Way of Tomoe (Reservoir)

Bar 907110, entity 1120830; event 11125830 uses cutscene 11020040 and event 11125840 cutscene 11020030. Defeat sets 9311.

Gate: Return to the Reservoir. Opens: Isshin, the Sword Saint; Required bosses by ending.

Confidence: data.

### Step 55. Isshin, the Sword Saint

Entity 1120860, bar 905400, events 11125860/70. Defeat sets 9312 and flag 11125706, plays achievement 13, and pays lots 2130 (Dragon Flash, goods 2460) and 2131 (Memory: Saint Isshin, goods 5211).

Gate: Genichiro, Way of Tomoe (Reservoir). Opens: The final choice and offering; Required bosses by ending.

Confidence: data.

### Step 56. The final choice and offering

After 9312, talk flags 11125707, 11125708 and 11125709 pick the ending cutscene in event 11120110 of m11_02. What decides the branch is what the player offers.

Gate: Isshin, the Sword Saint. Opens: Ending: Immortal Severance; Ending: Purification; Ending: Return.

Confidence: data.

### Step 57. Ending: Immortal Severance

Dragon's Tears alone with the Immortal Severance text: cutscene 11020100, flag 22, ending flag 6832, achievement 11.

Gate: The final choice and offering. Opens: The four endings in one table.

Confidence: data.

### Step 58. Ending: Purification

Dragon's Tears plus Aromatic Flower (Owl Father memory). Likely one of the two cutscene branches (11020110 flag 20 or 11020120/21 flag 21).

Gate: The final choice and offering. Opens: The four endings in one table.

Note: Which of flags 20 and 21 maps to Purification is not proven from data.

Confidence: open-question.

### Step 59. Ending: Return

Needs Frozen Tears, Rice for Kuro and the Divine Child questline done before the Dragon dies. Cutscene is one of 11020110 or 11020120/21.

Gate: Return questline items; The final choice and offering. Opens: The four endings in one table.

Note: Branch to cutscene mapping open.

Confidence: open-question.

### Step 60. After the credits: New Game Plus

Clear flag 6901 triggers common event 50 to reset key items and quest flags. Events 295/297/298 award Another's Memory (goods 9810 Ashina, 9800 Shura, 9820 Tengu) on NG+.

Gate: none beyond the start of the game. Opens: Inner bosses in New Game Plus.

Confidence: data.

## The four endings

| Ending | Last boss | Offering | Cutscene | Ending flag | Achievement |
|---|---|---|---|---|---|
| Shura | Isshin Ashina (9316) | none, chosen at the castle | 11010070 | 6833 | 12 |
| Immortal Severance | Isshin, the Sword Saint (9312) | Divine Dragon's Tears alone, with the Immortal Severance text | 11020100 | 6832 | 11 |
| Purification | Isshin, the Sword Saint (9312) | Dragon's Tears and the Aromatic Flower | 11020110 or 11020120 then 11020121 | 6830 or 6831 | 9 or 10 |
| Return | Isshin, the Sword Saint (9312) | Dragon's Tears, Frozen Tears and the Rice for Kuro questline | 11020110 or 11020120 then 11020121 | 6830 or 6831 | 9 or 10 |

Which of 6830 and 6831 is Purification and which is Return is not proven from the data. All four lead to the credits (atlas ids `frontend.credits-movie`, `frontend.ending-shura`, `frontend.ending-reservoir`).

## Required bosses

| Boss | Defeat flag | Needed for |
|---|---|---|
| Gyoubu Masataka Oniwa | 9301 | every ending |
| Lady Butterfly | 9302 | every ending |
| Genichiro Ashina (castle top) | 9303 | every ending |
| Great Shinobi Owl | 9308 | every ending |
| Emma, the Gentle Blade | 9315 | Shura |
| Isshin Ashina | 9316 | Shura |
| True Corrupted Monk | 9309 | the three Fountainhead endings |
| Divine Dragon | 9310 | the three Fountainhead endings |
| Genichiro, Way of Tomoe | 9311 | the three Fountainhead endings |
| Isshin, the Sword Saint | 9312 | the three Fountainhead endings |

Optional: Guardian Ape (9304), Folding Screen Monkeys (9305), Corrupted Monk (9306) are optional as fights but guard the Lotus, the Mortal Blade and the Mibu region; Headless Ape (9307), Demon of Hatred (9313), Owl Father (9317), all mini-bosses and the Inner bosses of New Game Plus.

## Tables and rules

### Required bosses by ending

Every ending needs Gyoubu, Lady Butterfly, Genichiro (castle) and the Great Shinobi Owl on the main road. Shura then needs Emma and Isshin Ashina. The other three endings need the True Corrupted Monk, the Divine Dragon, the Genichiro duel in the Reservoir and Isshin the Sword Saint. The Mortal Blade, Lotus and Shelter Stone gates pull in the Folding Screen Monkeys and Guardian Ape on the way.

Values: Common: 9301 Gyoubu, 9302 Butterfly, 9303 Genichiro, 9308 Owl. Shura: 9315 Emma, 9316 Isshin Ashina. Others: 9309 True Monk, 9310 Divine Dragon, 9311 Genichiro Tomoe, 9312 Isshin Sword Saint.

Note: Whether the Guardian Ape, Monkeys and Corrupted Monk are 'required' depends on the gate they guard; the data only forces the three castle-invasion items (step 36).

### Optional bosses

Optional: the Headless Ape (9307), the Demon of Hatred (9313), the Owl Father memory fight (9317), the Corrupted Monk if the Shelter Stone can be reached without it, and all mini-bosses. The Inner bosses (Inner Genichiro, Inner Father, Inner Isshin) belong to New Game Plus.

Values: flags 9307, 9313, 9317; NG+ lots 5770 (goods 5220), 5780 (goods 5221), 5790 (goods 5222).

### Prayer Beads and boss memories

Each boss pays a named Memory (goods 5200-5213) through lot x1 and a generic Memory (goods 5400) at x2; four Memories make one attack power point. Prayer Beads (goods 4000) come from lots such as 2015 and 2016 (Headless Ape) and from mini-bosses; four beads make a vitality necklace.

Values: Memories: 5200 Gyoubu (lot 2001), 5201 Butterfly (2031), 5202 Genichiro (2081), 5203 Monkeys (2021), 5204 Guardian Ape (2011), 5205 Corrupted Monk (2051), 5206 Owl (2091), 5207 Owl Father (2101), 5208 True Monk (2061), 5209 Divine Dragon (2071), 5210 Demon of Hatred (2041), 5211 Isshin Sword Saint (2131), 5212 Isshin Ashina (2121), 5213 Headless Ape (2017); Prayer Bead 4000; Gourd Seed 4400.

### World-state change table

The story is held in a few stage flags: 8306 prologue, 8301 castle invaded, 8305 castle after Genichiro, 8302 Divine Dragon down, 8303 Headless appears, 8304 second Hirata memory, 8307 Burrow idol off. Common events 910, 911, 912 and 920 set them from the items and boss flags.

Values: 8301 (event 910: goods 2400+2500+2501), 8302 (event 911: 9310), 8303 (event 912: 9304 on, 9314 off), 8304 (event 11105631 on talk flag 71100599), 8305 (m11_01 event 11115800), 8306 (event 50, cleared by 11100100), 8307 (event 920).

### Key items that gate progress

Gate items and where each is paid: Gun Fort Shrine Key (9405, lot 1110860), Mortal Blade (2400, lot 3020 and 62200), Lotus of the Palace (2500, lot 1700000), Shelter Stone (2501, lot 1500050 / 3030), Secret Passage Key (9404, lot 62306), Hidden Temple Key (9403, lot 61200), Gatehouse Key (9402, lot 10101400), Dragon's Tears (9000, lot 2070), Aromatic Flower (2503, lot 2100), Frozen Tears (9091, lot 62212), Rice for Kuro (9090, lot 62210), Water of the Palace (9070, lot 2500650).

Values: goods and lots as listed; common event 910 reads 2400, 2500, 2501.

### The four endings in one table

Shura: 9316 then cutscene 11010070 (flag 6833, achievement 12). Immortal Severance: Tears alone, cutscene 11020100 (6832, ach 11). Purification: Tears plus Aromatic Flower. Return: Tears plus Frozen Tears and the Child questline. The latter two use cutscenes 11020110 or 11020120 and 11020121 (6830 or 6831, ach 9 or 10).

Values: flags 6830-6833; achievements 9-12; offerings: goods 9000, 2503, 9091; cutscenes 11010070, 11020100, 11020110, 11020120, 11020121.

Note: Which of 6830 and 6831 is Purification is not proven from the data (open).

### The four regions can be done in a free order

Senpou, Sunken Valley, Ashina Depths and Mibu are not forced into order by flags. Each gives one castle item or key; the data only requires all three items at once for the invasion (8301). The recorded run does them in one order and many players use others.

Values: gate items 2400, 2500, 2501; no region flag read by the others.

### Inner bosses in New Game Plus

The Inner Genichiro, Inner Father and Inner Isshin fights replace the earlier versions in New Game Plus. Their rewards are lots 5770, 5780 and 5790 (Memories goods 5220-5222).

Values: ItemLotParam 5770, 5780, 5790 and generic 5771/5781/5791.

### When the player can return to the temple hub

The idol at the temple is always usable; stage 8302 turns off the Outskirts idols, and the Sculptor moves with 8306 and the later story. The hub's sky follows the clock.

Values: idol flag 11100000; stage flags 8306, 8302.
