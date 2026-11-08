#!/usr/bin/env node
"use strict";

/**
 * postinstall script — downloads and verifies the native SkillDo binary.
 * Published assets: macOS (aarch64), Windows (x64 / arm64), Linux (x64).
 * Linux arm64 has no prebuilt CLI yet and must be built from source.
 *
 * Environment variables:
 *   SKILLDO_DOWNLOAD_BASE  — override download URL base (for mirrors / CI)
 *   SKILLDO_REPO           — override GitHub repo (default: yancongya/skilldo)
 *   SKILLDO_SKIP_DOWNLOAD  — set to "1" to skip download (useful in CI)
 */

const { execFileSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");
const http = require("http");
const crypto = require("crypto");

const BIN_DIR = __dirname;
const REPO = process.env.SKILLDO_REPO || "yancongya/skilldo";
const BASE = process.env.SKILLDO_DOWNLOAD_BASE || `https://github.com/${REPO}/releases/latest/download`;
const API_BASE = `https://api.github.com/repos/${REPO}`;
const RELEASES_URL = `https://github.com/${REPO}/releases/latest`;
const HAS_CURL = (() => {
  try { execFileSync("curl", ["--version"], { stdio: "ignore" }); return true; } catch { return false; }
})();

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

// Keep in sync with npm/bin/skilldo.js: macOS always uses the aarch64 binary
// (Intel Macs run it through Rosetta 2), Windows uses x64 / arm64.
function getArch() {
  if (process.platform === "darwin") return "aarch64";
  const a = process.arch;
  if (a === "arm64" || a === "aarch64") return "arm64";
  if (a === "x64" || a === "x86_64") return "x64";
  throw new Error(`Unsupported architecture: ${a}`);
}

function getAssetName(platform, arch) {
  if (platform === "windows") return `skilldo-cli-windows-${arch}.zip`;
  if (platform === "macos") return "skilldo-cli-macos-aarch64.tar.gz";
  if (platform === "linux") {
    // The release-cli-linux workflow job publishes this asset for x64.
    if (arch === "x64") return "skilldo-cli-linux-x64.tar.gz";
    throw new Error(
      `No prebuilt Linux ${arch} CLI yet; build from source: git clone https://github.com/${REPO}.git && cd skilldo && npm install && ./scripts/build.sh cli`
    );
  }
  throw new Error(`No prebuilt CLI asset for ${platform}`);
}

function getBinaryName(platform, arch) {
  return platform === "windows"
    ? `skilldo-windows-${arch}.exe`
    : `skilldo-${platform}-${arch}`;
}

// curl handles proxies, redirects, retries and timeouts; fall back to Node's
// built-in https for minimal environments where curl is unavailable.
function downloadOne(url, dest) {
  if (HAS_CURL) {
    return Promise.resolve().then(() => {
      execFileSync("curl", [
        "-fL", "--silent", "--show-error",
        "--connect-timeout", "20", "--max-time", "180", "--retry", "2",
        "-H", "Accept: application/octet-stream",
        "-H", "User-Agent: skilldo-installer",
        url, "-o", dest,
      ], { stdio: ["ignore", "pipe", "pipe"] });
    });
  }
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    const get = url.startsWith("https") ? https.get : http.get;
    const request = get(url, { headers: { "User-Agent": "skilldo-installer" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        file.close();
        try { fs.unlinkSync(dest); } catch {}
        return downloadOne(res.headers.location, dest).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        file.close();
        try { fs.unlinkSync(dest); } catch {}
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      res.pipe(file);
      file.on("finish", () => { file.close(); resolve(); });
      file.on("error", (err) => { try { fs.unlinkSync(dest); } catch {} reject(err); });
    });
    request.on("error", (err) => { try { fs.unlinkSync(dest); } catch {} reject(err); });
    request.setTimeout(60000, () => request.destroy(new Error("request timed out")));
  });
}

function fetchJson(url) {
  if (HAS_CURL) {
    const out = execFileSync("curl", [
      "-fsSL", "--connect-timeout", "20", "--max-time", "60", "--retry", "2",
      "-H", "Accept: application/vnd.github+json",
      "-H", "User-Agent: skilldo-installer", url,
    ], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    return Promise.resolve(JSON.parse(out));
  }
  return new Promise((resolve, reject) => {
    const get = url.startsWith("https") ? https.get : http.get;
    const request = get(url, { headers: { "User-Agent": "skilldo-installer", "Accept": "application/vnd.github+json" } }, (res) => {
      if (res.statusCode !== 200) { res.resume(); return reject(new Error(`HTTP ${res.statusCode} for ${url}`)); }
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (chunk) => { body += chunk; });
      res.on("end", () => { try { resolve(JSON.parse(body)); } catch (e) { reject(e); } });
    });
    request.on("error", reject);
    request.setTimeout(60000, () => request.destroy(new Error("request timed out")));
  });
}

let cachedAssets = null;
async function apiAssetUrl(name) {
  if (process.env.SKILLDO_DOWNLOAD_BASE) return null;
  try {
    if (!cachedAssets) cachedAssets = (await fetchJson(`${API_BASE}/releases/latest`)).assets;
    const match = (cachedAssets || []).find((a) => a.name === name);
    return match ? `${API_BASE}/releases/assets/${match.id}` : null;
  } catch {
    return null;
  }
}

async function downloadAsset(name, dest) {
  const urls = [`${BASE}/${name}`];
  const fallback = await apiAssetUrl(name);
  if (fallback) urls.push(fallback);
  let lastError;
  for (const url of urls) {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        await downloadOne(url, dest);
        if (fs.existsSync(dest) && fs.statSync(dest).size > 0) return;
      } catch (err) {
        lastError = err;
        try { fs.unlinkSync(dest); } catch {}
      }
    }
  }
  throw new Error(`failed to download ${name} from the release CDN and the GitHub API asset endpoint (${lastError ? lastError.message : "unknown error"})`);
}

function verifyChecksum(archivePath, checksumPath) {
  const expected = fs.readFileSync(checksumPath, "utf8").trim().split(/\s+/)[0].toLowerCase();
  const actual = crypto.createHash("sha256").update(fs.readFileSync(archivePath)).digest("hex");
  if (expected !== actual) {
    throw new Error(`SHA-256 verification failed: expected ${expected}, got ${actual}`);
  }
}

async function main() {
  const platform = getPlatform();
  if (platform === "linux" && getArch() === "arm64") {
    console.error("skilldo: no prebuilt Linux arm64 CLI is published in GitHub Releases yet.");
    console.error(`skilldo: build from source: git clone https://github.com/${REPO}.git && cd skilldo && npm install && ./scripts/build.sh cli`);
    process.exitCode = 1;
    return;
  }

  const arch = getArch();
  const asset = getAssetName(platform, arch);
  const binaryName = getBinaryName(platform, arch);
  const binaryPath = path.join(BIN_DIR, binaryName);

  if (fs.existsSync(binaryPath)) {
    console.log(`skilldo: binary already present at ${binaryPath}`);
    return;
  }

  const tmpDir = path.join(BIN_DIR, `.tmp-${Date.now()}`);
  fs.mkdirSync(tmpDir, { recursive: true });

  try {
    const archivePath = path.join(tmpDir, asset);
    const checksumPath = `${archivePath}.sha256`;
    console.log(`skilldo: downloading ${asset}...`);
    await downloadAsset(asset, archivePath);
    await downloadAsset(`${asset}.sha256`, checksumPath);
    verifyChecksum(archivePath, checksumPath);
    console.log("skilldo: SHA-256 verified.");

    if (platform === "windows") {
      execFileSync("powershell", ["-NoProfile", "-Command", `Expand-Archive -Path '${archivePath}' -DestinationPath '${tmpDir}' -Force`], { stdio: "pipe" });
      fs.copyFileSync(path.join(tmpDir, "skilldo.exe"), binaryPath);
    } else {
      execFileSync("tar", ["-xzf", archivePath, "-C", tmpDir], { stdio: "pipe" });
      fs.copyFileSync(path.join(tmpDir, "skilldo"), binaryPath);
      fs.chmodSync(binaryPath, 0o755);
    }
    console.log(`skilldo: installed to ${binaryPath}`);
  } finally {
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }
}

main().catch((err) => {
  console.error(`skilldo: failed to install the native binary: ${err.message}`);
  console.error(`skilldo: you can install manually from ${RELEASES_URL}`);
  if (process.platform === "linux") {
    console.error("skilldo: releases created before the Linux build job have no Linux CLI asset; build from source with ./scripts/build.sh cli");
  }
  process.exitCode = 1;
});
