#!/usr/bin/env node
/**
 * 品牌改名脚本(去 Paste 化改名专用)
 *
 * 用法:
 *   node scripts/rename-brand.mjs --name ClipKit                  # 预览(dry-run)
 *   node scripts/rename-brand.mjs --name ClipKit --apply          # 实际写入
 *   node scripts/rename-brand.mjs --name ClipKit --domain clipkit.app --apply
 *
 * 参数:
 *   --name     新品牌名(必填,如 ClipKit)
 *   --repo     新 GitHub 仓库名(默认:品牌名小写)
 *   --domain   若提供,同时把 Tauri identifier 改为反转域名(如 clipkit.app → app.clipkit)
 *   --apply    真正写入文件;不带此项只预览替换数量
 *
 * 说明(重要):
 *   1. Rust crate 名(paste-next)、lib 名与数据库文件名 paste-next.db 保持不变 ——
 *      改动它们会改变应用数据目录,导致老用户的历史记录"消失"。
 *   2. 若使用 --domain 修改了 identifier,应用数据目录会随之变化,
 *      需要在新版首次启动时做一次数据迁移,否则老数据仍在旧目录。
 *   3. 改名后请重新生成图标:pnpm icon
 *   4. 改名后需同步更新 GitHub 仓库名(若改了 --repo)与 Pages 站点地址。
 */

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) continue;
    const key = a.slice(2);
    const next = argv[i + 1];
    if (!next || next.startsWith("--")) {
      args[key] = true;
    } else {
      args[key] = next;
      i++;
    }
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
const newName = typeof args.name === "string" ? args.name.trim() : "";

if (!newName) {
  console.error("错误:缺少 --name 参数\n");
  console.error("示例: node scripts/rename-brand.mjs --name ClipKit --apply");
  process.exit(1);
}

const repo = (typeof args.repo === "string" && args.repo) || newName.toLowerCase();
const apply = args.apply === true;
const domain = typeof args.domain === "string" ? args.domain : "";

const TARGETS = [
  "package.json",
  "src-tauri/tauri.conf.json",
  "src/i18n/zh.ts",
  "src/i18n/en.ts",
  "site/index.html",
  "scripts/install-app.sh",
  ".github/workflows/build.yml",
  ".github/workflows/pages.yml",
  "README.md",
  "README.en.md",
  "PRIVACY.md",
  "TERMS.md",
];

function reverseDomain(d) {
  const clean = d.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  return clean.split(".").reverse().join(".");
}

function transform(path, content) {
  let out = content;
  const counts = { brand: 0, repo: 0, identifier: 0 };

  // 品牌名(大小写敏感优先)
  const brandRe = /PasteNext/g;
  counts.brand = (out.match(brandRe) || []).length;
  out = out.replace(brandRe, newName);

  // 仓库名 / 域名 / 小写标识
  const repoRe = /pastenext/g;
  counts.repo = (out.match(repoRe) || []).length;
  out = out.replace(repoRe, repo);

  // Tauri identifier(仅 tauri.conf.json,且仅当提供 --domain)
  if (domain && path.endsWith("tauri.conf.json")) {
    const idRe = /"identifier"\s*:\s*"[^"]+"/;
    if (idRe.test(out)) {
      out = out.replace(idRe, `"identifier": "${reverseDomain(domain)}"`);
      counts.identifier = 1;
    }
  }

  return { out, counts };
}

console.log(`品牌改名: PasteNext → ${newName}${domain ? ` · identifier → ${reverseDomain(domain)}` : ""}`);
console.log(`模式: ${apply ? "写入(apply)" : "预览(dry-run,加 --apply 才会写文件)"}\n`);

let totalFiles = 0;
let totalHits = 0;

for (const rel of TARGETS) {
  const abs = join(root, rel);
  if (!existsSync(abs)) {
    console.log(`- ${rel} (不存在,跳过)`);
    continue;
  }
  const before = readFileSync(abs, "utf8");
  const { out, counts } = transform(rel, before);
  const hits = counts.brand + counts.repo + counts.identifier;
  if (hits === 0) {
    console.log(`- ${rel} 无匹配`);
    continue;
  }
  totalFiles++;
  totalHits += hits;
  console.log(
    `✓ ${rel}  品牌 ${counts.brand} 处 · 仓库/小写 ${counts.repo} 处${counts.identifier ? " · identifier 1 处" : ""}`
  );
  if (apply) writeFileSync(abs, out, "utf8");
}

console.log(`\n合计:${totalFiles} 个文件,${totalHits} 处替换。`);

if (!apply) {
  console.log("\n这是一次预览。确认无误后加 --apply 执行写入。");
} else {
  console.log("\n已写入。接下来请执行:");
  console.log("  1. pnpm icon          # 重新生成图标(若图标内含品牌字样)");
  console.log("  2. pnpm build         # 校验前端编译");
  console.log("  3. 重命名 GitHub 仓库并更新 Pages 地址(若改了 --repo)");
  console.log("  4. 检查 README / 官网中的历史截图与链接");
}
