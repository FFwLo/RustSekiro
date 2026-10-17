# Boss audit: Sv1 vs the live recordings

Made by `python tools/boss_audit.py --run --secs 240`: Sv1 plays each fight headless, every phase 240 s against an unkillable Wolf who walks up and swings every 1.5 s, and 8 s of every 20 keeps 15 m off (the boss keeps full health and posture), then a break + deathblow. Wolf never guards, heals, jumps or uses tools there, so moves the AI keeps for those show up as missing.

## Folding Screen Monkeys (row 12600050, c1260)

Recordings: rec_20261015_233452_boss_monkeys, rec_20261015_234545_boss_monkeys_b (698 s, 3 deathblows on the boss).
Sv1: 1 phases, 0 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 12501099 (c1250): 444 anims; model yes, cloth yes, data yes, in Sv1 fight NO
- 12600000 (c1260): 26 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 12600010 (c1260): 318 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 12600020 (c1260): 29 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 12600030 (c1260): 6 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 12600050 (c1260): 58 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 12200080 73001080 74700081)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 12501099: Sekiro 21, Sv1 0, both 0; Sekiro only: 003000 003003 003004 003005 003006 003007 003010 003011 003015 003016 003017 003018 003021 005201 005202 005210 005211 005212 005400 005401 005402
- 12600020: Sekiro 2, Sv1 3, both 1; Sekiro only: 005000

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (12600000): 11260005/11260006: a232_502500 a232_512500 a000_014500
- Sekiro 2 (12600020): 11260005/11260006: a232_502500 a232_512500 a000_014500
- Sekiro 3 (12600010): 11260005/11260006: a232_502500 a232_512500 a000_014500
- Sv1 1: c1260 a000_012000; wolf a202_500000 a202_510000

**Bullets**: Sekiro 1 rows, Sv1 0; Sekiro only: 12504100; Sv1 only: -

**Events**: Sv1 script has 29; 18 of them ran in Sekiro; Sv1 ran 11.
- ran in Sekiro, not in Sv1: 12005210 (2003:4 2005:3), 12005800 (2003:12 2003:2 2003:28 2003:88 2004:15 2004:4 2004:8 2005:3 2006:1 2006:3 2010:2 2012:1), 12005820 (2004:8 2005:3 2006:3), 12005827 (2004:21 2004:8), 12005828 (2004:21 2004:8), 12005830 (2004:21 2004:24 2004:8), 12005835 (2004:21 2004:50), 12005845 (2004:21 2004:8), 12005858 (2004:8), 12005870 (2004:19)

**Arena**: Sv1 None (None, radius 140 m). Sekiro boss from its middle: 95% within 132 m, max 132 m, height range 33 m. Wolf: 95% within 105 m, max 122 m, height range 168 m. Sv1 boss: 95% within 54 m, height range 0 m.

## Corrupted Monk (illusion) (row 50001000, c5000)

Recordings: rec_20261016_000242_boss_corrupted_monk (328 s, 4 deathblows on the boss).
Sv1: 1 phases, 27 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50001000 (c5000): 425 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 50001000: Sekiro 35, Sv1 19, both 19; Sekiro only: 003002 003015 003021 003050 003051 003055 003060 003061 003062 003064 003065 003066 003072 003076 003077 003101

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (50001000): 15000190: a211_511800 a000_013800
- Sekiro 2 (50001000): 15000190: a211_511800 a000_013800
- Sekiro 3 (50001000): 15000190: a211_511800 a000_013800
- Sekiro 4 (50001000): 15000120/15000121: a211_511100 a000_013100 a211_511110 a000_013110
- Sv1 1: c5000 a000_012000 a000_020000; wolf a211_500000 a211_510000

**Bullets**: Sekiro 1 rows, Sv1 1; Sekiro only: -; Sv1 only: -

**Events**: Sv1 script has 4; 4 of them ran in Sekiro; Sv1 ran 3.
- ran in Sekiro, not in Sv1: 11505814 (2003:31)

**Arena**: Sv1 boss_50001000 (m15_00_00_00, radius 60 m). Sekiro boss from its middle: 95% within 21 m, max 25 m, height range 4 m. Wolf: 95% within 17 m, max 25 m, height range 7 m. Sv1 boss: 95% within 46 m, height range 0 m.

## True Corrupted Monk (row 50000000, c5000)

Recordings: rec_20261016_002433_boss_true_monk (859 s, 15 deathblows on the boss).
Sv1: 4 phases, 104 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50000000 (c5000): 971 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 50010000 (c5001): 78 anims; model yes, cloth none, data yes, in Sv1 fight yes
- 50050000 (c5005): 416 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 50000000: Sekiro 59, Sv1 27, both 23; Sekiro only: 003018 003025 003027 003050 003051 003056 003060 003061 003062 003064 003065 003066 003067 003069 003072 003076 003077 003101 005202 005203 003003 003004 003005 003007 003008 003009 003022 003035 003036 003037 003040 003061 003062 003065 003066 003069
- 50050000: Sekiro 6, Sv1 3, both 3; Sekiro only: 003024 003025 003026

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 2 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 3 (50000000): 15000010/15000011: a211_510100 a000_012100 a211_510110 a000_012110
- Sekiro 4 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 5 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 6 (50000000): 15000011: a211_510100 a000_012100 a000_012110 a211_510110
- Sekiro 7 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 8 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 9 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 10 (50000000): 15000160/15000161: a211_511500 a000_013500 a211_511510 a000_013510
- Sekiro 11 (50000000): 15000190: a211_511800 a000_013800
- Sekiro 12 (50000000): 15000120/15000121: a211_511100 a000_013100 a211_511110 a000_013110
- Sekiro 13 (50000000): 15000190: a211_511800 a100_013800
- Sekiro 14 (50000000): 15000160/15000161: a211_511500 a100_013500 a211_511510 a100_013510
- Sekiro 15 (50000000): 15000180/15000181: a211_501700 a211_511700 a100_013700
- Sv1 1: c5000 a000_012000; c5005 a200_003006 a200_003011 a200_003023 a200_405010; wolf a211_500000 a211_510000
- Sv1 2: c5000 a000_012000 a000_020010; wolf a211_500000 a211_510000
- Sv1 3: c5000 a000_012000 a100_020200; wolf a211_500000 a211_510000
- Sv1 4: c5005 a100_020200

**Bullets**: Sekiro 4 rows, Sv1 1; Sekiro only: 50000400 50000401 50000402; Sv1 only: -

**Events**: Sv1 script has 14; 13 of them ran in Sekiro; Sv1 ran 13.

**Arena**: Sv1 boss_50000000 (m25_00_00_00, radius 60 m). Sekiro boss from its middle: 95% within 23 m, max 24 m, height range 2 m. Wolf: 95% within 22 m, max 39 m, height range 79 m. Sv1 boss: 95% within 49 m, height range 0 m.

## Great Shinobi Owl (row 50600000, c5060)

Recordings: rec_20261016_001225_boss_owl (594 s, 2 deathblows on the boss).
Sv1: 3 phases, 164 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50600000 (c5060): 682 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10006000 10010000 50600080 54300000 72001000 74000010 74000080)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 50600000: Sekiro 31, Sv1 17, both 17; Sekiro only: 003007 003017 003030 003040 003045 003060 003061 003065 003066 003068 003069 003101 003103 004100

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (50600000): 15060010/15060011: a221_510100 a000_012100 a221_510110 a000_012110
- Sekiro 2 (50600000): 15060000/15060001: a221_500000 a221_510000 a000_012000
- Sv1 1: c5060 a000_012000; wolf a221_500000 a221_510000
- Sv1 2: c5060 a000_012000 a000_020200; wolf a221_500000 a221_510000

**Bullets**: Sekiro 27 rows, Sv1 3; Sekiro only: 50600255 50600256 50600257 50600258 50600259 50600260 50600261 50600701 50600702 50600721 50600731 50600732 50600741 50600742 50601131 50601250 50601251 50601252 50601253 50601254 50601700 50601720 50601730 50601740; Sv1 only: -

**Events**: Sv1 script has 9; 6 of them ran in Sekiro; Sv1 ran 6.
- ran in Sekiro, not in Sv1: 11115873 (2006:1 2006:2)

**Arena**: Sv1 boss_71001000 (m11_01_00_00, radius 60 m). Sekiro boss from its middle: 95% within 14 m, max 19 m, height range 0 m. Wolf: 95% within 14 m, max 17 m, height range 2 m. Sv1 boss: 95% within 65 m, height range 0 m.

## Owl (Father) (row 50601010, c5060)

Recordings: rec_20261016_074559_boss_owl_father (1008 s, 4 deathblows on the boss).
Sv1: 4 phases, 77 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50601010 (c5060): 1285 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 50700000 (c5070): 350 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000 15100200 15100300 15100400 50900000 72100000)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 50601010: Sekiro 48, Sv1 29, both 28; Sekiro only: 003006 003007 003026 003028 003034 003035 003036 003038 003040 003041 003042 003060 003061 003063 003064 003090 003101 003103 004100 004110
- 50700000: Sekiro 14, Sv1 13, both 13; Sekiro only: 003007

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (50601010): 15060010/15060011: a221_510100 a000_012100 a221_510110 a000_012110
- Sekiro 2 (50601010): 15060190: a221_511800
- Sekiro 3 (50601010): 15060000/15060001: a221_500000 a221_510000 a000_012000
- Sekiro 4 (50601010): 15060180/15060181: a221_501700 a221_511700 a000_013700
- Sv1 1: c5060 a000_012000 a000_020001; c5070 a000_020000 a000_020001; wolf a221_500000 a221_510000
- Sv1 2: c5060 a000_012000 a000_020001 a000_020200; c5070 a000_020001; wolf a221_500000 a221_510000
- Sv1 3: c5060 a000_012000 a000_020001 a000_020200; c5070 a000_020000 a000_020001; wolf a221_500000 a221_510000

**Bullets**: Sekiro 29 rows, Sv1 23; Sekiro only: 50600131 50600191 50600192 50600193 50600194 50600986; Sv1 only: -

**Events**: Sv1 script has 12; 10 of them ran in Sekiro; Sv1 ran 12.

**Arena**: Sv1 boss_71001000 (m11_01_00_00, radius 60 m). Sekiro boss from its middle: 95% within 24 m, max 30 m, height range 2 m. Wolf: 95% within 23 m, max 29 m, height range 5 m. Sv1 boss: 95% within 4439 m, height range 6 m.

## Gyoubu Oniwa (row 50800000, c5080)

Recordings: rec_20261015_210505_boss_gyoubu, rec_20261015_211200_boss_gyoubu_b, rec_20261015_211958_boss_gyoubu_c (596 s, 6 deathblows on the boss).
Sv1: 2 phases, 102 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50800000 (c5080): 572 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000 76000081)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 50800000: Sekiro 36, Sv1 26, both 23; Sekiro only: 003001 003005 003011 003012 003013 003032 003041 003046 003064 003089 003097 004100 005010

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (50800000): 15080110/15080111: a225_501200 a225_511200 a000_013200
- Sekiro 2 (50800000): 15080009: a225_510090
- Sekiro 3 (50800000): 15080000/15080001: a225_500000 a225_510000 a000_012000
- Sekiro 4 (50800000): 15080170/15080171: a225_511600 a000_013600 a225_511610 a000_013610
- Sekiro 5 (50800000): 15080000/15080001: a225_510000 a000_012000
- Sekiro 6 (50800000): 15080009: a225_510090
- Sv1 1: c5080 a000_012000; wolf a225_500000 a225_510000
- Sv1 2: c5080 a000_012090; wolf a225_500000 a225_510090

**Bullets**: Sekiro 3 rows, Sv1 4; Sekiro only: -; Sv1 only: 50800981

**Events**: Sv1 script has 11; 8 of them ran in Sekiro; Sv1 ran 6.
- ran in Sekiro, not in Sv1: 11105845 (2004:21 2004:8), 20006002 (2000:5 2003:62)

**Arena**: Sv1 boss_54000000 (m11_02_00_00, radius 60 m). Sekiro boss from its middle: 95% within 44 m, max 52 m, height range 6 m. Wolf: 95% within 41 m, max 46 m, height range 9 m. Sv1 boss: 95% within 109 m, height range 0 m.

## Lady Butterfly (row 50900000, c5090)

Recordings: rec_20261015_225710_boss_butterfly (333 s, 2 deathblows on the boss).
Sv1: 2 phases, 71 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 15100200 (c1510): 44 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 15100300 (c1510): 49 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 15100400 (c1510): 25 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 50900000 (c5090): 137 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 50900001 (c5090): 171 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000 50601010 50700000 72100000)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 15100200: Sekiro 3, Sv1 0, both 0; Sekiro only: 003000 003001 003031
- 15100300: Sekiro 5, Sv1 0, both 0; Sekiro only: 003031 003000 003001 003003 003025
- 15100400: Sekiro 3, Sv1 0, both 0; Sekiro only: 003010 003011 003020
- 50900000: Sekiro 28, Sv1 24, both 19; Sekiro only: 003003 003017 003060 003061 003103 003202 003232 003242 004100
- 50900001: Sekiro 28, Sv1 24, both 16; Sekiro only: 003003 003018 003019 003021 003060 003061 003065 003103 003202 003232 003242 005201

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (50900000): 15090010/15090011: a239_510100 a000_012100 a239_510110 a000_012110 a239_510111 a000_012111
- Sekiro 2 (50900001): 15090680/15090681: a244_501700 a244_511700 a100_013700
- Sv1 1: c5090 a000_012000 a000_012001 a100_021000; wolf a239_500000 a239_510000
- Sv1 2: c5090 a100_012000 a100_020200; wolf a244_500000 a244_510000

**Bullets**: Sekiro 14 rows, Sv1 6; Sekiro only: 50900330 50900331 50900420 50900421 50900430 50900920 50900921 50900982 50900983; Sv1 only: 50900100

**Events**: Sv1 script has 15; 13 of them ran in Sekiro; Sv1 ran 4.
- ran in Sekiro, not in Sv1: 11005800 (2000:5 2002:13 2003:12 2003:2 2003:23 2003:28 2004:15 2004:21 2004:39 2004:5 2004:8 2006:1 2008:4), 11005820 (2003:18 2003:54 2004:1 2004:11 2004:17 2004:20 2004:40 2004:5), 11005850 (2006:2), 11005851 (2003:2), 11005855 (2003:2 2004:6), 11005857 (2004:12), 11005860 (2003:2), 11005870 (2004:4), 11005880 (2004:17), 11005881 (2003:22 2004:8)

**Arena**: Sv1 boss_50900000 (m10_00_00_00, radius 60 m). Sekiro boss from its middle: 95% within 20 m, max 24 m, height range 5 m. Wolf: 95% within 18 m, max 27 m, height range 4 m. Sv1 boss: 95% within 4641 m, height range 5 m.

## Guardian Ape (row 51000000, c5100)

Recordings: rec_20261015_234913_boss_guardian_ape (572 s, 3 deathblows on the boss).
Sv1: 2 phases, 338 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50011000 (c5001): 29 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 51000000 (c5100): 396 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 51000000: Sekiro 35, Sv1 30, both 28; Sekiro only: 004100 003001 003016 003065 003067 003068 003069

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (51000000): 15100180/15100181: a230_501700 a230_511700 a000_013700
- Sekiro 2 (51000000): 15100500/15100501: a235_510000 a100_012000 a235_510001
- Sekiro 3 (51000000): 15100509: a235_510090 a100_012090
- Sv1 1: c5100 a000_012000 a000_020200; wolf a230_500000 a230_510000
- Sv1 2: c5100 a100_012090; wolf a235_500000 a235_510090

**Bullets**: Sekiro 7 rows, Sv1 7; Sekiro only: -; Sv1 only: -

**Events**: Sv1 script has 20; 16 of them ran in Sekiro; Sv1 ran 13.
- ran in Sekiro, not in Sv1: 11705823 (2004:12), 11705825 (2004:39 2004:6 2004:8 2010:2), 11705827 (2004:21 2004:8), 11705833 (2004:21 2004:8)

**Arena**: Sv1 boss_51000000 (m17_00_00_00, radius 60 m). Sekiro boss from its middle: 95% within 38 m, max 43 m, height range 1 m. Wolf: 95% within 35 m, max 44 m, height range 15 m. Sv1 boss: 95% within 111 m, height range 0 m.

## Headless Ape (row 51000100, c5100)

Recordings: rec_20261016_004230_boss_headless_ape (417 s, 3 deathblows on the boss).
Sv1: 2 phases, 566 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 50011000 (c5001): 14 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 51000010 (c5100): 159 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 51000100 (c5100): 289 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000 10802000)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 51000010: Sekiro 8, Sv1 19, both 2; Sekiro only: 003006 003007 003019 003020 003023 003029
- 51000100: Sekiro 19, Sv1 19, both 14; Sekiro only: 003002 003017 003063 003066 003069

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (51000100): 15100500/15100501: a235_510000 a100_012000
- Sekiro 2 (51000010): 15100000/15100001: a230_500000 a230_510000 a000_012000
- Sekiro 3 (51000100): 15100500/15100501/15100509: a235_500000 a235_510000 a100_012000 a235_510001
- Sv1 1: c5100 a100_012000; wolf a235_500000 a235_510000
- Sv1 2: c5100 a100_012090; wolf a235_500000 a235_510090

**Bullets**: Sekiro 2 rows, Sv1 2; Sekiro only: -; Sv1 only: -

**Events**: Sv1 script has 16; 10 of them ran in Sekiro; Sv1 ran 7.
- ran in Sekiro, not in Sv1: 11705872 (2004:6 2004:8), 11705875 (2004:21 2004:8), 11705884 (2004:17 2004:20 2004:39 2004:5), 11705886 (2004:21 2004:8)

**Arena**: Sv1 boss_51000100 (m17_00_00_00, radius 60 m). Sekiro boss from its middle: 95% within 31 m, max 37 m, height range 2 m. Wolf: 95% within 30 m, max 35 m, height range 2 m. Sv1 boss: 95% within 159 m, height range 0 m.

## Old Dragons / Divine Dragon (row 53000000, c5300)

Recordings: rec_20261016_070725_boss_divine_dragon (578 s, 7 deathblows on the boss).
Sv1: 12 phases, 392 hits on Wolf, NOT won (the run stopped first).

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 52000000 (c5200): 97 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 53000010 (c5300): 36 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 53000011 (c5300): 16 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 53000012 (c5300): 17 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 53100001 (c5310): 73 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 53100021 (c5310): 1965 anims; model yes, cloth none, data yes, in Sv1 fight NO

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 52000000: Sekiro 17, Sv1 14, both 7; Sekiro only: 003032 003034 003035 003038 003039 003040 003045 003046 003047 003049
- 53000010: Sekiro 1, Sv1 6, both 0; Sekiro only: 003008
- 53000012: Sekiro 1, Sv1 6, both 1
- 53100021: Sekiro 2, Sv1 0, both 0; Sekiro only: 003021 003000

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (53000012): 15300000/15300001: a231_510000
- Sekiro 2 (53000010): 15300000/15300001: a231_500000 a231_510000
- Sekiro 3 (53000010): 15300000: a231_500000 a000_010010
- Sekiro 4 (53000010): 15300040/15300041: a231_510400 a000_012400 a000_012410 a231_510410
- Sekiro 5 (53000011): 15300000/15300001: a231_500000 a231_510000
- Sekiro 6 (53000011): 15300040/15300041: a231_510400 a231_510410 a000_012410
- Sekiro 7 (52000000): 15200090: a233_510900 a000_012900
- Sv1 1: c5300 a000_012000 a000_012001; c5310 a000_020000 a000_020004 a000_021000 a000_021002; wolf a231_500000 a231_510000
- Sv1 2: no throw anims (the forced deathblow did not land)
- Sv1 3: no throw anims (the forced deathblow did not land)
- Sv1 4: no throw anims (the forced deathblow did not land)
- Sv1 5: no throw anims (the forced deathblow did not land)
- Sv1 6: no throw anims (the forced deathblow did not land)
- Sv1 7: no throw anims (the forced deathblow did not land)
- Sv1 8: no throw anims (the forced deathblow did not land)
- Sv1 9: no throw anims (the forced deathblow did not land)
- Sv1 10: no throw anims (the forced deathblow did not land)
- Sv1 11: no throw anims (the forced deathblow did not land)
- Sv1 12: no throw anims (the forced deathblow did not land)

**Bullets**: Sekiro 55 rows, Sv1 25; Sekiro only: 52000550 52000555 52000610 52000611 52000612 52000613 52000660 52000661 52000665 52000700 52000820 53100600 53100610 53100620 53100630 53100661 53100662 53100663 53100664 53100665 53100671 53100672 53100673 53100674 53100675 53100681 53100682 53100683 53100684 53100685; Sv1 only: -

**Events**: Sv1 script has 30; 26 of them ran in Sekiro; Sv1 ran 12.
- ran in Sekiro, not in Sv1: 12505803 (2003:2 2003:31 2004:11 2004:26 2004:8), 12505840 (2003:2 2010:2), 12505851 (2004:30), 12505880 (2000:5 2003:11 2003:2 2003:23 2003:28 2003:74 2003:79 2003:92 2003:93 2004:12 2004:8 2006:3 2008:4), 12505887 (2003:18 2003:2 2003:5), 12505889 (2003:79 2004:34 2004:8), 12505890 (2004:17 2004:8), 12505891 (2003:17 2003:22 2003:5 2004:17 2004:42), 12505897 (2003:5), 12505906 (2005:19 2005:3), 12505911 (2003:2 2003:5 2004:17 2004:29 2004:39 2004:5), 12505916 (2003:2 2003:5), 12505921 (2003:5), 12505927 (2003:18), 12505929 (2004:42)

**Arena**: Sv1 boss_52000000 (m25_00_00_00, radius 140 m). Sekiro boss from its middle: 95% within 58 m, max 61 m, height range 0 m. Wolf: 95% within 63 m, max 82 m, height range 79 m. Sv1 boss: 95% within 47 m, height range 0 m.

## Way of Tomoe / Isshin, the Sword Saint (row 71101000, c7110)

Recordings: rec_20261016_080607_boss_tomoe_genichiro_isshin (962 s, 18 deathblows on the boss).
Sv1: 6 phases, 471 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 54000000 (c5400): 974 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 71101000 (c7110): 326 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10000000 72000200)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 54000000: Sekiro 66, Sv1 48, both 43; Sekiro only: 003003 003013 003015 003023 003027 003028 003044 003045 003060 003070 003071 003072 003077 003101 005201 005202 003006 003011 003014 003019 003031 003101 005202
- 71101000: Sekiro 35, Sv1 22, both 19; Sekiro only: 003007 003009 003025 003032 003034 003062 003063 003067 003068 003076 003092 003101 003103 004110 005201 005202

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (71101000): 17110190: a228_511800 a000_013800
- Sekiro 2 (71101000): 17110120/17110121: a228_511100 a000_013100 a228_511110 a000_013110
- Sekiro 3 (54000000): 15400190: a242_511800 a000_013800
- Sekiro 4 (54000000): 15400190: a242_511800 a000_013800
- Sekiro 5 (54000000): 15400190: a000_013800 a242_511800
- Sekiro 6 (54000000): 15400190: a242_511800 a000_013800
- Sekiro 7 (54000000): 15400000/15400001: a242_500000 a242_510000 a000_012000
- Sekiro 8 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 9 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 10 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 11 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 12 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 13 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 14 (54000000): 15400510/15400511: a243_510100 a100_012100 a243_510110 a100_012110
- Sekiro 15 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 16 (54000000): 15400690: a243_511800 a100_013800
- Sekiro 17 (54000000): 15400500/15400501: a243_510000 a100_012000
- Sekiro 18 (54000000): 15400590: a243_510900 a100_012900
- Sv1 1: c7110 a000_012000; wolf a228_500000 a228_510000
- Sv1 2: c5400 a000_012000; wolf a242_500000 a242_510000
- Sv1 3: c5400 a100_012000; wolf a243_500000 a243_510000
- Sv1 4: no throw anims (the forced deathblow did not land)
- Sv1 5: c5400 a100_012000 a100_020200; wolf a243_500000 a243_510000
- Sv1 6: c5400 a100_012000 a100_020200; wolf a243_500000 a243_510000

**Bullets**: Sekiro 26 rows, Sv1 15; Sekiro only: 54000270 54000271 54000272 54000273 54000274 54000275 54000800 71100980 71100981 71100983 71100984; Sv1 only: -

**Events**: Sv1 script has 10; 9 of them ran in Sekiro; Sv1 ran 7.
- ran in Sekiro, not in Sv1: 11120110 (2003:2 2004:8), 11125733 (2003:2), 11125860 (2003:2 2003:28 2003:74 2003:88 2006:1 2006:2 2008:4 2010:3 2011:1 2012:1)

**Arena**: Sv1 boss_71001000 (m11_01_00_00, radius 60 m). Sekiro boss from its middle: 95% within 29 m, max 36 m, height range 3 m. Wolf: 95% within 28 m, max 38 m, height range 6 m. Sv1 boss: 95% within 4978 m, height range 1 m.

## Emma / Isshin Ashina (row 74000010, c7400)

Recordings: rec_20261016_082459_boss_emma_isshin_ashina (596 s, 6 deathblows on the boss).
Sv1: 12 phases, 601 hits on Wolf, NOT won (the run stopped first).

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 54300000 (c5430): 469 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 74000010 (c7400): 238 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10006000 10010000 50600000 50600080 72001000 74000080)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 54300000: Sekiro 36, Sv1 27, both 22; Sekiro only: 003003 003009 003013 003018 003025 003026 003037 003038 003060 003070 003071 003077 003101 003102
- 74000010: Sekiro 21, Sv1 16, both 11; Sekiro only: 003021 003025 003026 003061 003066 003067 003070 003102 004100 005202

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (74000010): 17400190: a246_511800 a000_013800
- Sekiro 2 (74000010): 17400000/17400001: a246_510000 a000_012000
- Sekiro 3 (54300000): 15430190: a242_511800 a000_013800
- Sekiro 4 (54300000): 15430010/15430011: a242_510100 a000_012100 a242_510110 a000_012110
- Sekiro 5 (54300000): 15430000/15430001: a242_510000 a000_012000
- Sekiro 6 (54300000): 15430180/15430181: a242_501700 a000_013700 a242_511700
- Sv1 1: c7400 a000_012000 a000_012001; wolf a246_500000 a246_510000
- Sv1 2: wolf a242_600390
- Sv1 3: no throw anims (the forced deathblow did not land)
- Sv1 4: no throw anims (the forced deathblow did not land)
- Sv1 5: wolf a242_600390
- Sv1 6: no throw anims (the forced deathblow did not land)
- Sv1 7: no throw anims (the forced deathblow did not land)
- Sv1 8: no throw anims (the forced deathblow did not land)
- Sv1 9: no throw anims (the forced deathblow did not land)
- Sv1 10: no throw anims (the forced deathblow did not land)
- Sv1 11: no throw anims (the forced deathblow did not land)
- Sv1 12: no throw anims (the forced deathblow did not land)

**Bullets**: Sekiro 2 rows, Sv1 12; Sekiro only: 74000987; Sv1 only: 54000360 54000361 54000362 54000370 54000371 54000382 54000383 54000980 54000981 74000982 74000984

**Events**: Sv1 script has 10; 9 of them ran in Sekiro; Sv1 ran 6.
- ran in Sekiro, not in Sv1: 11110100 (2002:13 2003:18 2003:2 2003:28 2003:4 2003:76 2003:80 2003:81 2003:82 2003:88 2004:15 2004:21 2004:29 2004:5 2004:8 2005:3 2006:1 2012:1), 11115920 (2003:12 2003:2 2003:28 2004:4 2008:4), 11115941 (2004:6), 11115942 (2004:12)

**Arena**: Sv1 boss_71001000 (m11_01_00_00, radius 60 m). Sekiro boss from its middle: 95% within 14 m, max 17 m, height range 0 m. Wolf: 95% within 14 m, max 19 m, height range 3 m. Sv1 boss: 95% within 31 m, height range 0 m.

## Demon of Hatred (row 70200000, c7020)

Recordings: rec_20261016_072651_boss_demon_of_hatred (952 s, 4 deathblows on the boss).
Sv1: 3 phases, 290 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 10010000 (c1001): 41 anims; model yes, cloth none, data yes, in Sv1 fight NO
- 70200000 (c7020): 736 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 17001010 17001210 76000081)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 70200000: Sekiro 29, Sv1 30, both 27; Sekiro only: 003006 003047

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (70200000): 17020000/17020001: a240_500000 a240_510000 a000_012000
- Sekiro 2 (70200000): 17020000/17020001: a240_500000 a240_510000 a000_012000
- Sekiro 3 (70200000): 17020000/17020001: a240_510000 a000_012000
- Sekiro 4 (70200000): 17020180/17020181: a240_501700 a240_511700 a000_013700
- Sv1 1: c7020 a000_012000 a000_020001; wolf a240_500000 a240_510000
- Sv1 2: c7020 a000_012000; wolf a240_500000 a240_510000
- Sv1 3: c7020 a000_012000; wolf a240_500000 a240_510000

**Bullets**: Sekiro 37 rows, Sv1 29; Sekiro only: 70200231 70200232 70200234 70200350 70200356 70200364 70200365 70200366; Sv1 only: -

**Events**: Sv1 script has 13; 10 of them ran in Sekiro; Sv1 ran 7.
- ran in Sekiro, not in Sv1: 11105900 (2003:12 2003:2 2003:28 2004:5 2005:19 2005:3 2008:4), 11105920 (2004:12), 11105921 (2004:6), 20006041 (2004:42)

**Arena**: Sv1 boss_54000000 (m11_02_00_00, radius 60 m). Sekiro boss from its middle: 95% within 53 m, max 68 m, height range 6 m. Wolf: 95% within 54 m, max 62 m, height range 8 m. Sv1 boss: 95% within 69 m, height range 0 m.

## Genichiro (Outskirts, first) (row 71000000, c7100)

Recordings: rec_20261015_193250_boss_genichiro_first, rec_20261015_193824_boss_genichiro_first_b, rec_20261015_210022_boss_genichiro_first_c (365 s, 5 deathblows on the boss).
Sv1: 2 phases, 128 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 71000000 (c7100): 535 anims; model yes, cloth yes, data yes, in Sv1 fight yes

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 71000000: Sekiro 29, Sv1 16, both 16; Sekiro only: 003000 003004 003025 003028 003055 003065 003067 003068 003071 003092 003101 003103 004100

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (71000000): 17100190: a227_511800 a000_013800
- Sekiro 2 (71000000): 17100190: a227_511800 a000_013800
- Sekiro 3 (71000000): 17100190: a227_511800 a000_013800
- Sekiro 4 (71000000): 17100000/17100001/17100190: a000_013800 a227_511800 a227_500000 a227_510000 a000_012000
- Sekiro 5 (71000000): 17100190: a227_511800 a000_013800
- Sv1 1: c7100 a000_012000; wolf a227_500000 a227_510000
- Sv1 2: c7100 a000_012000; wolf a227_500000 a227_510000

**Bullets**: Sekiro 1 rows, Sv1 1; Sekiro only: -; Sv1 only: -

**Events**: Sv1 script has 3; 2 of them ran in Sekiro; Sv1 ran 2.
- ran in Sekiro, not in Sv1: 11125800 (2000:5 2002:13 2003:11 2003:2 2003:22 2003:23 2004:21 2006:1)

**Arena**: Sv1 boss_71001000 (m11_01_00_00, radius 60 m). Sekiro boss from its middle: 95% within 20 m, max 25 m, height range 4 m. Wolf: 95% within 21 m, max 26 m, height range 6 m. Sv1 boss: 95% within 172 m, height range 0 m.

## Genichiro (castle top) / Way of Tomoe (row 71001000, c7100)

Recordings: rec_20261015_231740_boss_genichiro_castle, rec_20261015_232705_boss_genichiro_castle_b (686 s, 18 deathblows on the boss).
Sv1: 3 phases, 223 hits on Wolf, won.

**Characters** (Sekiro row: anims seen; Sv1 model / cloth / data / in the Sv1 fight):
- 71001000 (c7100): 826 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- 71100000 (c7110): 335 anims; model yes, cloth yes, data yes, in Sv1 fight yes
- (near the fight, never attacked: 10010000 54300000 72001000 74000000 74000010 74000080)

**Attacks** (a000_003xxx-005xxx; Sekiro played, Sv1 never did):
- 71001000: Sekiro 37, Sv1 26, both 22; Sekiro only: 003013 003023 003028 003031 003036 003044 003055 003065 003067 003068 003071 003092 003101 003103 004100
- 71100000: Sekiro 28, Sv1 24, both 14; Sekiro only: 003019 003029 003031 003032 003036 003041 003050 003067 003068 003080 003092 003101 003103 005201

**Deathblows** (ThrowParam rows: the throw anims after):
- Sekiro 1 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 2 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 3 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 4 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 5 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 6 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 7 (71001000): 17100120/17100121: a227_511100 a000_013100 a227_511110 a000_013110
- Sekiro 8 (71100000): 17110190: a228_511800 a000_013800
- Sekiro 9 (71100000): 17110000/17110001: a228_500000 a228_510000 a000_012000
- Sekiro 10 (71100000): 17110180/17110181: a228_501700 a228_511700 a000_013700
- Sekiro 11 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 12 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 13 (71001000): 17100190: a227_511800 a000_013800
- Sekiro 14 (71001000): 17100000/17100001: a227_500000 a227_510000 a000_012000
- Sekiro 15 (71001000): 17100010/17100011: a227_510100 a000_012100 a227_510110 a000_012110
- Sekiro 16 (71100000): 17110190: a228_511800 a000_013800
- Sekiro 17 (71100000): 17110010/17110011: a228_510100 a000_012100 a228_510110 a000_012110
- Sekiro 18 (71100000): 17110180/17110181: a228_501700 a228_511700 a000_013700
- Sv1 1: c7100 a000_012000; wolf a227_500000 a227_510000
- Sv1 2: c7100 a000_012000; wolf a227_500000 a227_510000
- Sv1 3: c7110 a000_012000; wolf a228_500000 a228_510000

**Bullets**: Sekiro 17 rows, Sv1 12; Sekiro only: 71000510 71000530 71000531 71000532 71000533 71000540 71100985; Sv1 only: 71100980 71100981

**Events**: Sv1 script has 6; 5 of them ran in Sekiro; Sv1 ran 2.
- ran in Sekiro, not in Sv1: 11115800 (2002:13 2003:12 2003:18 2003:2 2003:28 2003:81 2003:82 2003:88 2004:15 2004:29 2004:8 2005:3 2006:1 2006:2 2008:4 2012:1), 11115830 (2004:6), 11115831 (2004:12)

**Arena**: Sv1 boss_71001000 (m11_01_00_00, radius 60 m). Sekiro boss from its middle: 95% within 18 m, max 22 m, height range 0 m. Wolf: 95% within 15 m, max 19 m, height range 3 m. Sv1 boss: 95% within 4991 m, height range 1 m.

