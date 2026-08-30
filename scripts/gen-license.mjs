#!/usr/bin/env node
/**
 * PasteNext 序列号生成器。
 *
 * 算法必须与 src-tauri/src/license.rs 完全一致,两边各有一份独立实现,
 * 任何一侧改动都要同步 —— 否则新发的号在客户端验不过。
 *
 * 用法:
 *   单个       node scripts/gen-license.mjs someone@example.com
 *   多个       node scripts/gen-license.mjs a@x.com b@y.com
 *   批量文件   node scripts/gen-license.mjs --file customers.txt   # 每行一个邮箱
 *
 * 环境变量(正式发版必填,必须与 CI 中构建应用时用的是同一对值):
 *   PASTENEXT_SIGN_SECRET   签名密钥
 *   PASTENEXT_MAIL_SECRET   邮箱绑定密钥
 *
 * 未设置时使用开发占位密钥,只能激活本地 debug 构建。
 */
import { createHmac } from "node:crypto";
import { readFileSync } from "node:fs";

const SIGN_SECRET = process.env.PASTENEXT_SIGN_SECRET || "dev-sign-secret-never-ship";
const MAIL_SECRET = process.env.PASTENEXT_MAIL_SECRET || "dev-mail-secret-never-ship";

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"; // Crockford:去掉 I L O U
const KEY_VERSION = 1;
const KEY_CHARS = 16;

const normalizeEmail = (email) => email.trim().toLowerCase();

function hmac(key, msg) {
  return createHmac("sha256", key).update(msg).digest();
}

function b32Encode(bytes) {
  let out = "";
  let bits = 0;
  let nbits = 0;
  for (const b of bytes) {
    bits = (bits << 8) | b;
    nbits += 8;
    while (nbits >= 5) {
      nbits -= 5;
      out += ALPHABET[(bits >> nbits) & 0x1f];
    }
  }
  if (nbits > 0) out += ALPHABET[(bits << (5 - nbits)) & 0x1f];
  return out;
}

// 10 字节 = 80 bits,Base32 恰好 16 个字符,无填充位。
// 长度必须与 src-tauri/src/license.rs 的 PAYLOAD_LEN / SIGNED_LEN 保持一致。
const PAYLOAD_LEN = 10;
const SIGNED_LEN = 6;

export function generateKey(email) {
  const mail = hmac(MAIL_SECRET, normalizeEmail(email)).subarray(0, 4);
  const payload = Buffer.alloc(SIGNED_LEN);
  payload[0] = KEY_VERSION;
  mail.copy(payload, 1);
  payload[5] = 0; // flags 预留
  const sig = hmac(SIGN_SECRET, payload).subarray(0, 4);
  const full = Buffer.concat([payload, sig]); // 10 字节
  const s = b32Encode(full);
  if (s.length !== KEY_CHARS) {
    throw new Error(`编码长度异常:期望 ${KEY_CHARS} 字符,得到 ${s.length}`);
  }
  // 每 4 个字符一段
  return s.match(/.{4}/g).join("-");
}

// ---- CLI ----
const args = process.argv.slice(2);
if (args.length === 0 || args.includes("--help") || args.includes("-h")) {
  console.log(`用法: node scripts/gen-license.mjs <邮箱> [更多邮箱...]
       node scripts/gen-license.mjs --file customers.txt`);
  process.exit(args.length === 0 ? 1 : 0);
}

let emails = args;
if (args[0] === "--file") {
  const file = args[1];
  if (!file) {
    console.error("缺少 --file 参数后的文件路径");
    process.exit(1);
  }
  emails = readFileSync(file, "utf8")
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("#"));
}

if (SIGN_SECRET.startsWith("dev-") || MAIL_SECRET.startsWith("dev-")) {
  console.error(
    "⚠️  正在使用开发占位密钥。生成的序列号只能激活本地 debug 构建。\n" +
      "   正式发号前请设置 PASTENEXT_SIGN_SECRET 与 PASTENEXT_MAIL_SECRET。"
  );
}

console.log("");
for (const email of emails) {
  console.log(`${email}\t${generateKey(email)}`);
}
console.log("");
console.log(`共生成 ${emails.length} 个序列号。`);
