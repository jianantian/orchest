import { copyFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const candidates = [
  "agent_runtime_node.node",
  "libagent_runtime_node.dylib",
  "libagent_runtime_node.so",
  "agent_runtime_node.dll",
];

const releaseDir = join(process.cwd(), "target", "release");
const source = candidates.map((name) => join(releaseDir, name)).find(existsSync);

if (!source) {
  throw new Error(`Could not find agent-runtime-node build output in ${releaseDir}`);
}

copyFileSync(source, join(process.cwd(), "agent_runtime_node.node"));
