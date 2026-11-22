# Bug fixes

Robigo copies S2's behaviour exactly, including a handful of outright bugs in `S2.EXE` that
only showed up when long computer games were traced against the original. Each of those bugs
sits behind one setting, `BugFixes` in the game's `Setup`:

- **On** (the default for a new game): each bug below plays as the original meant it to.
- **Off**: the game matches S2 step for step, bugs and all.

The options window shows the setting as "Disable bug fixes". It applies to the next new game.
A game keeps the setting it started with for its whole life: replays write it as a
`bug-fixes on` or `bug-fixes off` line and saves carry it in their replay. A replay or save
without the line plays with the fixes off, since every game recorded before the setting existed
played the bugs.

The S2 trace test (`crates/s2-sim/tests/ai_trace.rs`), the recorded replays and the scenario
tests that pin S2's behaviour all run with the fixes off. Each bug has a test of each side.

## The bugs

Quirks that are part of how S2 plays (settlers getting lost, ships taking odd routes) are not
bugs in this sense and play the same either way.

| Bug | With the fixes off, as S2 | With the fixes on |
|---|---|---|
| Figure pool growth (`0x32b34`, `0x78788`) | The figure loop reads the pool's address once. When a figure's turn grows the pool, the loop walks on through the freed block and every later figure misses the step. | Every figure takes its turn. |
| Freed helper record (`0x25b9c`, `0x36ebc`) | A removed road makes lost the helper its carrier order names, even when that helper's record is already free. The fill bytes read as player 0's metalworker on a node no list holds, which then wanders as a lost settler, and player 0 loses a scythe. | A free record is left alone. |
| Pack donkey lost in a doorway (`0x36fe6`) | A carrier made lost while it walks through a building's door with a ware becomes a helper whatever it was, so a pack donkey comes home as a helper. | A pack donkey stays a pack donkey. |
| Capture's land count (`0x304d4`, `0x304e2`) | A capture moves a fixed 9 from the old owner's land count to the new owner's while it hands over ten protected nodes, so both counts end one node off the map for good. The AI reads that count for its thresholds. | The nodes that change hands are counted one by one. |
| Job 30 settler home (`0x3165c`) | A settler walking into a storehouse with a look the returns table has no row for is counted at its own job number, past the 30 job counts. Job 30 lands on the ware stop bits, so the first one home stops wood at that storehouse. | It counts nothing. |
| AI site search run out (`0x6a93e`, `0x22498`) | A site search that reaches ring 12 reports failure but stays on the running list. Every later turn steps its spiral with a side counter past 5, which the cursor step ignores, so it tests the node where ring 12 ended for the rest of the game and places a building there if that node ever passes. | The search leaves the running list once it has reported. |
| Fight ending on the soldier's own flag (`0x37f78`, `0x33894`) | A soldier whose fight ends on its home's flag searches a path from that flag to itself, which always fails, so it is lost instead of walking in. | It walks in. |
| Forgetting a figure (`0x3b165`) | When a soldier dies, walks in, is lost or is sent home, every soldier record naming it as a target is cleared. The test meant for defenders lets every record through, so an attacker whose target building has the same number as that figure loses its target too. | Only defenders' records are cleared. |
| Attackers that lose their target in their door (`0x368d5`, `0x36134`) | An attacker whose target is gone by the time it reaches its own flag walks back in and is deleted. Its home still counts it as asked for and never asks again, so the building stays short of a soldier. | It walks back in and joins its home. |
| Demolition and settlers walking home (`0x2c65f`, `0x2c8c2`) | Demolition reads a settler's countdown byte as its workplace, so a lost settler walking home whose countdown happens to equal the building's number is made lost again. Helpers escape only because the job test skips job 0. | Settlers walking home carry on. |
| Stranded ware out of the flag search's reach (`0x2ab7d`) | A ware whose building goes while it waits is stranded at the flag the search finds it at: its next flag or a flag one road from it. When it is at neither, it is given no flag, so the stranded-ware pass never routes it again and it sits there for good. | It is stranded at the flag whose list holds it. |

## Fixed outright

Two bugs in the Gold Edition's World Campaign check (`0x2a1a0`) are fixed whatever the setting,
since they change only who wins a continent and nothing a trace reads:

| Bug | `S2.EXE` | Robigo |
|---|---|---|
| A computer's win | Any player that is the last with a headquarters wins, so a computer that takes the person's headquarters marks the continent won in `MISSION2.DAT`. | Only a player a person controls can win. |
| A win that repeats | The check never tests the word that says the game is decided, so every 160 steps after a win it opens the winner's window again and writes `MISSION2.DAT` again. | The check stops once the game is decided, as the other objectives do. |

The observation window's pick of a figure to follow (`0x48db0`) is fixed whatever the setting
too, since it changes only what one window shows:

| Bug | `S2.EXE` | Robigo |
|---|---|---|
| Following a figure out of sight | The window follows the figure nearest its middle wherever that figure is. With every near figure on land the player has not seen, the picture is dark, and the button that moves the main view there gives away where an opponent's settler walks. | Only figures on nodes the player sees count, a figure that walks out of sight is let go, and with none in sight the picture stays on its node. |

## Adding one

A bug found by tracing is copied first, so the trace test matches with the fixes off. It then
goes behind `World::bug_fixes`, with the fixed behaviour when the setting is on, a test of each
side and a row in the table above.
