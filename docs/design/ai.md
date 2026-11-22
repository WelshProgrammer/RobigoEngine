# Computer players

How Robigo runs its computer players: the Original, S2's own AI rebuilt to match S2 exactly, and
AIs that mods write in Lua. Both answer to one set of queries and actions. The Original runs
inside the simulation, as S2's does. A Lua AI runs beside it and hands back commands, like a person
at the keyboard.

## What S2 does

S2's AI is a job system that polls. It has no event hooks: every step the sim calls it at a fixed
point, after the catapult stones and before the animal spawner, in three parts:

1. A tool pass every 480 steps, for each player the computer plays, which sets the tool weights
   from what is wanted. It needs no AI object.
2. A grid pass every step, which rebuilds 4 of the 8 × 8-node cells the AI scores the map by.
3. At most one player's think a step: player `p` thinks when the step counter modulo 10 is `p`.
   Every fourth of those thinks, so every 40 steps, its 22 tasks look at the game and queue jobs.
   Every think then starts the new jobs and moves one running job on.

The AI acts on the game by calling the world's own routines in the middle of the step: placing
buildings and flags, laying a road along a list of nodes, demolishing, launching attacks, writing
the distribution, tool and military settings. It skips checks a person's command faces. A
placement never asks whether the type is allowed yet, and an attack can send a count the attack
button would never offer. It draws from the game's one generator, at game creation and while it
thinks.

Every slot in the game gets an AI object, people's included, and creating one draws once from the
generator. Only slots the computer plays think.

## Two hosts, one vocabulary

A computer player is written against two traits in `s2-sim`:

- `Sight`: what a player may ask of the game. Its nodes, owners and seen land; its buildings,
  flags, roads, stock, settings and statistics; its letters. It reads and never changes anything.
  The Original's own view adds what S2's AI reads that a person cannot: the grid survey and the ore
  under unexplored land, which S2 reads straight from the map without a geologist.
- `Hands`: what a player may do. The commands a person has, plus the actions only the Original
  uses: a road laid along a given list of nodes, and a placement or attack without the person's
  checks.

The Original's logic is written once, generic over both. The two hosts give it different `Hands`:

| | The Original | A Lua AI |
|---|---|---|
| Runs | inside `Game::step`, at S2's point in the step | after each step, outside the sim |
| Acts | at once, through the world's routines, as S2 does | by returning `Command`s, applied before the next step |
| Random numbers | the sim's generator, as S2 | its own generator, seeded per slot |
| Sees | what S2's AI reads, the whole map included | what its player has seen, as a person does |
| Privileged actions | yes, where S2 takes them | no: the checked `Command`s only |
| Commands a step | as many as S2's jobs make | at most the cap its difficulty sets |
| State | part of the game state: hashed, saved | its own file in the save, written by the script |
| In a replay | runs again, like the rest of the sim | does not run: its commands are in the replay |
| In multiplayer | runs on every machine, in lockstep | runs on the host, which sends its commands |

The Original has to sit inside the step. Its actions land mid-step and its draws move every later
draw: animals, fights, everything. Replayed as commands at step boundaries, they would land in the
wrong place and the generator would drift from S2 from the first frame.

A Lua AI only has to behave the same on the one machine that runs it. Floating-point maths and
Lua's own libraries are no risk to replays or to multiplayer, since neither reruns the script.

A slot whose AI is a Lua script still gets the Original's object at creation and makes its one
draw, so the other players' draws stay where they are in S2. It never thinks, and the tool pass
leaves it alone: the script sets its own tool weights.

## Fair play

A Lua AI plays by a person's rules. It sees what its player has seen: the land inside the fog and
whatever its letters report, and the whole map only when Exploration is off, as a person would. It
acts only through the `Command`s a person's clicks make, each checked as a person's is. The host
applies at most a set number of its commands each step, from the difficulty chosen for its slot,
and drops the rest:

| Difficulty | Commands a step | A second, at 7.5 steps |
|---|---|---|
| Easy | 2 | 15 |
| Normal | 5 | 37.5 |
| Hard | 10 | 75 |

A script with no cap plays at machine speed, which in other games has meant a whole network built
in the first second. The script is told its difficulty and its cap at `start`; whether it plays
easier at a lower setting is up to its author.

## Events

S2's AI polls, and the Original keeps S2's cadence exactly. A Lua AI is called after every step
with the step number and the letters its player has received since the last call (attacks,
buildings finished, land lost, finds), which are the events a person sees. The script decides how
often to think. A call that runs past its instruction budget is stopped and returns no commands.

## The Lua host

`s2-ai` holds the Lua host on `mlua`, with the standard libraries trimmed to what cannot reach the
machine: no `io`, `os`, `require` of files or `debug`. A script exports four functions:

```lua
function start(slot, sight, rules)   -- the game starts or a save loads; rules has the difficulty and the cap
function think(sight, letters)       -- after every step; returns a list of commands
function save_state(slot)            -- returns the bytes to keep
function restore_state(slot, bytes)  -- returns true, or nil and the reason it cannot use them
```

`sight` is the read-only view, and a command is a table naming one of `Command`'s variants and its
fields. The host keeps the script's generator: `ai.random(n)` draws from it, and its state goes in
the save with the script's own bytes.

An AI is a mod with an `ai/` folder holding its script and a name for the list. Several AIs can be
installed at once, and each slot picks one.

## Saves and replays

The slot line of a replay names the AI of each computer player when it is not the Original:
`slot 3 computer 7 ai=smarter-settlers`. A replay plays back without the script, from its
commands. The line is there to say what the player faced.

A save holds an entry `ai/<slot>.state` for each slot a Lua AI plays, with the bytes
`save_state` returned and the host's generator state. The Original's state is part of the game
state, as S2 keeps its AI in `COMPYnnn.DAT`. Loading a save is refused, naming the AI and the
slot, when:

- the AI's mod is not installed, which the mod check already finds;
- the script's `restore_state` turns down its bytes, for instance after a change to the script
  that its author did not carry the old state across. That is between the author and the script.

A save restored by replaying its commands, when its state format is from an older build, gets the
script's bytes back the same way once the replay reaches the saved step.

## The settings screen

Unlimited Play's settings screen shows S2's rows unchanged while the Original is the only AI
installed, so the screen matches S2 pixel for pixel. With a Lua AI installed, each row the
computer plays gains a button after the Mode column showing its AI's short name, "Original" by
default. It opens a list of the installed AIs, laid out like the map list: the names in a list, the
chosen one's author, version and a line of description under it, and Ok and Back. That list is
Robigo's own and goes in [ui.md](ui.md) as a departure.

## Proving the Original

The Original is right when a game with computer players reaches the same state as S2, step for
step. Two checks:

- The AI's pools and the world are read from the memory of S2 running under DOSBox-X, and a
  test plays the same map and setup in Robigo and compares them at each think: tasks, jobs, the grid
  and the generator's state.
- Headless games, computer against computer, on every shipped map, as determinism soak tests and
  benchmarks in CI, with their hashes pinned.

## Order of work

1. A map of S2's AI and this design.
2. The frame in `s2-sim`: the AI objects for every slot and their creation draw, the 300-job pool
   and its lists, the 22 tasks as stubs, the three passes at their place in the step, the state in
   the save. Hashes and replays prove that games without a computer player do not change.
3. The grid pass and the cell survey, and the tool pass.
4. The tasks, a group at a time, each checked against S2 under DOSBox-X: placement, flags and
   roads (tasks 0 to 4); expansion (6, 7 and 9); upkeep (10 to 12); attacks (13); the economy
   planner, its projects and the military sets (14 to 17).
5. The settings screen's AI list and the replay's `ai=` field.
6. `s2-ai` and the Lua host.

The first full skirmish against the Original comes at the end of step 4.

## Open

- Where the settings screen offers the difficulty.
- How the instruction budget is set and whether a mod can ask for more.
