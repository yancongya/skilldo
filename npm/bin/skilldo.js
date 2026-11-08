#!/usr/bin/env node
"use strict";

/**
 * SkillDo npm wrapper — proxies all arguments to the native Rust binary.
 * The actual binary is downloaded during `npm install` via postinstall.
 */

const { execFileSync } = require("child_process");
const path = require("path");
const fs = require("fs");

const BIN_DIR = path.join(__dirname, "..", "bin");
const PLATFORM = getPlatform();
const ARCH = getArch();
const BINARY = path.join(BIN_DIR, `skilldo-${PLATFORM}-${ARCH}${PLATFORM === "windows" ? ".exe" : ""}`);

function getPlatform() {
  const p = process.platform;
  if (p === "darwin") return "macos";
  if (p === "linux") return "linux";
  if (p === "win32") return "windows";
  throw new Error(`Unsupported platform: ${p}`);
}

// Keep in sync with npm/bin/install-binary.js: macOS always uses the aarch64
// binary (Intel Macs run it through Rosetta 2), Windows uses x64 / arm64.
function getArch() {
  if (process.platform === "darwin") return "aarch64";
  const a = process.arch;
  if (a === "arm64" || a === "aarch64") return "arm64";
  if (a === "x64" || a === "x86_64") return "x64";
  throw new Error(`Unsupported architecture: ${a}`);
}

if (!fs.existsSync(BINARY)) {
  console.error(
    `Error: SkillDo binary not found at ${BINARY}\n` +
    `Run 'npx skilldo postinstall' or reinstall the package.`
  );
  process.exit(1);
}

try {
  execFileSync(BINARY, process.argv.slice(2), { stdio: "inherit" });
} catch (err) {
  if (err.status !== undefined) {
    process.exit(err.status);
  }
  throw err;
}
