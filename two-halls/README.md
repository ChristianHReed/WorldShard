# Two Halls

A small real-time strategy game in the style of Age of Empires. Up to four people
share one screen. It is a separate game from WorldShard and shares no code with it.

**To play:** download `index.html` and open it in a browser. It is one
self-contained page with nothing to install.

## What is in it

- Two players (one at each end of the screen), a one-on-one computer opponent,
  and 3 or 4 players on tablet-sized or larger screens
- Gathering, building where you tap, walls and gates
- Docks, fishing boats, galleys and bombard ships
- Village, Town and Castle ages that unlock buildings, units and upgrades
- Map sizes from 32×48 to 128×192, and maps made from a picture

The in-game help on the home page explains the controls.

## Changing it

| Path | What it is |
|---|---|
| `src/game/content.ts` | Costs and stats for units, buildings and upgrades |
| `src/game/sim.ts` | Game rules |
| `src/game/ai.ts` | Computer opponent |
| `src/game/map.ts`, `mapgen.ts` | Built-in map and picture maps |
| `src/ui/` | Drawing, touch controls and menus |

After editing, rebuild `index.html` (needs Node.js):

```sh
cd two-halls
npm install
npm run build
```

The game logic began as a Grok-generated project and was finished with Claude.
