#!/usr/bin/env node

const { generateKeyPairSync } = require("node:crypto");

function base64UrlToBase64(value) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const paddingLength = (4 - (normalized.length % 4)) % 4;
  return normalized + "=".repeat(paddingLength);
}

function decodeBase64Url(value) {
  return Buffer.from(base64UrlToBase64(value), "base64");
}

function main() {
  const outputJson = process.argv.includes("--json");
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const privateJwk = privateKey.export({ format: "jwk" });
  const publicJwk = publicKey.export({ format: "jwk" });

  if (!privateJwk.d || !publicJwk.x) {
    throw new Error("failed to export Ed25519 keypair as JWK");
  }

  const privateSeed = decodeBase64Url(privateJwk.d);
  const publicBytes = decodeBase64Url(publicJwk.x);

  if (privateSeed.length !== 32) {
    throw new Error(
      `expected 32-byte Ed25519 private seed, received ${privateSeed.length} bytes`,
    );
  }
  if (publicBytes.length !== 32) {
    throw new Error(
      `expected 32-byte Ed25519 public key, received ${publicBytes.length} bytes`,
    );
  }

  const privateKeyBase64 = privateSeed.toString("base64");
  const publicKeyBase64 = publicBytes.toString("base64");

  if (outputJson) {
    process.stdout.write(
      JSON.stringify(
        {
          HUNK_UPDATE_PRIVATE_KEY_BASE64: privateKeyBase64,
          HUNK_UPDATE_PUBLIC_KEY: publicKeyBase64,
        },
        null,
        2,
      ) + "\n",
    );
    return;
  }

  process.stdout.write(
    [
      "# GitHub Actions secrets",
      `HUNK_UPDATE_PRIVATE_KEY_BASE64=${privateKeyBase64}`,
      `HUNK_UPDATE_PUBLIC_KEY=${publicKeyBase64}`,
      "",
      "# Local test usage",
      `export HUNK_UPDATE_PUBLIC_KEY='${publicKeyBase64}'`,
    ].join("\n") + "\n",
  );
}

try {
  main();
} catch (error) {
  console.error(`error: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
}
