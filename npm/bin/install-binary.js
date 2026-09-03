#!/usr/bin/env node
"use strict";

/**
 * postinstall script — downloads the native SkillDo binary for the current platform.
 * Supports: macOS (arm64), Linux (x64, aarch64), Windows (x64, arm64).
 *
 * Environment variables:
 *   SKILLDO_DOWNLOAD_BASE  — override download URL base (for mirrors / CI)
 *   SKILLDO_REPO           — override GitHub repo (default: yancongya/skilldo)
 *   SKILLDO_SKIP_DOWNLOAD  — set to "1" to skip download (useful in CI)
 */

const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");
const http = require("http");

const BIN_DIR = path.join(__dirname);
const REPO = process.env.SKILLDO_REPO || "yancongya/skilldo";
const BASE = process.env.SKILLDO_DOWNLOAD_BASE || `https://github.com/${REPO}/releases/latest/download`;

if (process.env.SKILLDO_SKIP_DOWNLOAD === "1") {
  console.log("skilldo: SKILLDO_SKIP_DOWNLOAD=1, skipping binary download.");
  process.exit(0);
}

function getPlatform() {
  const p = process.platform;
  if (p === "darwin") return "macos";
  if (p === "linux") return "linux";
  if (p === "win32") return "windows";
  throw new Error(`Unsupported platform: ${p}`);
}

function getArch() {
  const a = process.arch;
  if (a === "arm64" || a === "aarch64") return "aarch64";
  if (a === "x64" || a === "x86_64") return "x64";
  throw new Error(`Unsupported architecture: ${a}`);
}

function getAssetName(platform, arch) {
  if (platform === "windows") {
    return `skilldo-cli-windows-${arch === "arm64" ? "arm64" : "x64"}.zip`;
  }
  if (arch === "aarch64") return `skilldo-cli-${platform}-aarch64.tar.gz`;
  return `skilldo-cli-${platform}-x64.tar.gz`;
}

function getBinaryName(platform) {
  const arch = getArch();
  if (platform === "windows") return `skilldo-windows-${arch === "arm64" ? "arm64" : "x64"}.exe`;
  const suffix = arch === "aarch64" ? "aarch64" : "x64";
  return `skilldo-${platform}-${suffix}`;
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    const get = url.startsWith("https") ? https.get : http.get;
    get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        // Follow redirect
        file.close();
        fs.unlinkSync(dest);
        return download(res.headers.location, dest).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        file.close();
        fs.unlinkSync(dest);
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      res.pipe(file);
      file.on("finish", () => { file.close(); resolve(); });
      file.on("error", (err) => { fs.unlinkSync(dest); reject(err); });
    }).on("error", (err) => { fs.unlinkSync(dest); reject(err); });
  });
}

async function main() {
  const platform = getPlatform();
  const arch = getArch();
  const asset = getAssetName(platform, arch);
  const binaryName = getBinaryName(platform);
  const binaryPath = path.join(BIN_DIR, binaryName);

  // Skip if binary already exists
  if (fs.existsSync(binaryPath)) {
    console.log(`skilldo: binary already present at ${binaryPath}`);
    return;
  }

  const url = `${BASE}/${asset}`;
  const tmpDir = path.join(BIN_DIR, `.tmp-${Date.now()}`);
  fs.mkdirSync(tmpDir, { recursive: true });

  try {
    console.log(`skilldo: downloading ${asset}...`);
    const archivePath = path.join(tmpDir, asset);
    await download(url, archivePath);
    console.log(`skilldo: extracting...`);

    if (platform === "windows") {
      // Windows: unzip
      execSync(`powershell -Command "Expand-Archive -Path '${archivePath}' -DestinationPath '${tmpDir}' -Force"`, { stdio: "pipe" });
      fs.copyFileSync(path.join(tmpDir, "skilldo.exe"), binaryPath);
    } else {
      // Unix: tar
      execSync(`tar -xzf "${archivePath}" -C "${tmpDir}"`, { stdio: "pipe" });
      fs.copyFileSync(path.join(tmpDir, "skilldo"), binaryPath);
      fs.chmodSync(binaryPath, 0o755);
    }

    console.log(`skilldo: installed to ${binaryPath}`);
  } catch (err) {
    console.error(`skilldo: failed to download binary: ${err.message}`);
    console.error(`skilldo: you can install manually from https://github.com/${REPO}/releases`);
    // Don't fail the install — the user can still use 'npx skilldo' after manual binary placement
  } finally {
    // Cleanup temp dir
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }
}

main();
