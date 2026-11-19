// SPDX-License-Identifier: MIT
// The wasm board under Node: frames per second by the process's CPU time, best of ROUNDS.
//   node bench.mjs BOARD.wasm IMAGE.pce [tg16] [--frames N] [--rounds N]
//   node bench.mjs BOARD.wasm --levels        # the PSG level table's 128 entries as hex bits
import { readFileSync } from "node:fs";
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at < 0 ? fallback : Number(args.splice(at, 2)[1]);
};
const frames = option("--frames", 600), rounds = option("--rounds", 5);
const module = new WebAssembly.Module(readFileSync(args[0]));
if (args[1] === "--levels") {
  const { exports } = new WebAssembly.Instance(module, {});
  for (let u = 0; u < 128; u++) console.log(BigInt.asUintN(64, exports.level_bits(u)).toString(16).padStart(16, "0"));
  process.exit(0);
}
const image = readFileSync(args[1]);
let best = 0, check = 0, output = 0;
for (let round = 0; round < rounds; round++) {
  const { exports } = new WebAssembly.Instance(module, {});
  const at = exports.alloc(image.length);
  new Uint8Array(exports.memory.buffer, at, image.length).set(image);
  exports.start(at, image.length, args[2] === "tg16" ? 1 : 0);
  const began = process.cpuUsage();
  exports.run(frames);
  const spent = process.cpuUsage(began);
  check = exports.check() >>> 0;
  output = exports.output_check() >>> 0;
  best = Math.max(best, frames / ((spent.user + spent.system) / 1e6));
}
const name = args[1].split("/").pop().replace(/\.pce$/, "");
const hex = (value) => value.toString(16).toUpperCase().padStart(8, "0");
console.log(`${name}: ${best.toFixed(1)} frames/s, output ${hex(output)}, check ${hex(check)}`);
