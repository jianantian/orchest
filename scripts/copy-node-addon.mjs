import { copyFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const candidates = [
  "orchest_node.node",
  "liborchest_node.dylib",
  "liborchest_node.so",
  "orchest_node.dll",
];

const releaseDir = join(process.cwd(), "target", "release");
const source = candidates.map((name) => join(releaseDir, name)).find(existsSync);

if (!source) {
  throw new Error(`Could not find orchest-node build output in ${releaseDir}`);
}

copyFileSync(source, join(process.cwd(), "orchest_node.node"));
