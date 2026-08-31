import { readFile } from "node:fs/promises";

const tauriConfig = JSON.parse(
  await readFile("src-tauri/tauri.conf.json", "utf8"),
);
const packageJson = JSON.parse(await readFile("package.json", "utf8"));
const cargoToml = await readFile("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargoToml.match(/^version = "([^"]+)"/m)?.[1];

const versions = {
  tag: process.env.GITHUB_REF_NAME,
  tauri: tauriConfig.version,
  package: packageJson.version,
  cargo: cargoVersion,
};
const expectedTag = `v${versions.tauri}`;

if (
  versions.tauri !== versions.package ||
  versions.tauri !== versions.cargo ||
  versions.tag !== expectedTag
) {
  console.error(`Version mismatch: ${JSON.stringify(versions)}`);
  process.exit(1);
}

console.log(`Release version verified: ${expectedTag}`);
