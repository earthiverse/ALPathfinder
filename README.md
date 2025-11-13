# ALPathfinder

A Rust/WASM Pathfinder for Adventure.Land.

This project is a work in progress. It currently doesn't work.

## Build

1. Run `wasm-pack build` to build.
2. Add the following lines to `package.json`:

```js
  "main": "alpathfinder.js",
  "type": "module",
```

## Use Locally

1. In the node project you want to use the pathfinder, run `npm install alpathfinder@file:../path/to/alpathfinder`.
