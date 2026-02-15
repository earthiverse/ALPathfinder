# ALPathfinder

A Rust/WASM Pathfinder for Adventure.Land.

## Usage

1. Run `npm i alpathfinder` to install.
2. Import the pathfinder

```js
import * as ALPathfinder from "alpathfinder";
```

3. Prepare the pathfinder with values from `G` and an optional list of maps you wish to ignore.

```js
const ignoreMaps = [
  "abtesting",
  "bank_b", // NOTE: Don't ignore if you have access
  "bank_u", // NOTE: Don't ignore if you have access
  "cgallery",
  "d2",
  "d_e",
  "duelland",
  "shellsisland",
  "ship0",
  "test",
  "old_bank",
  "old_main",
  "original_main",
  "resort",
  "resort_e",
];
ALPathfinder.prepare(parent.G, ignoreMaps);
```

4. Use the pathfinder

```js
ALPathfinder.canWalkPath(character.map, character.x, character.y, 123, 456)
const path = ALPathfinder.getPath(character.map, character.x, character.y, "spookytown", 0, 0, character.speed);
pathfinder.isWalkable("main", 200, 20)
```

## Development

### Build

1. Run `wasm-pack build` to build.
2. Add `typed-adventureland` as a dependency

### Test

1. After building, install the package by referencing the `pkg` directory that was built `npm i ../path/to/alpathfinder/pkg`.
