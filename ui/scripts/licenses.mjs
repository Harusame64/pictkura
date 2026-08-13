// 配布物に入るnpmパッケージの著作権表示とライセンス条文を書き出す。
//
// cargo-about が見るのはRustの依存だけで、UIのバンドルに入るJavaScriptは
// 対象外になる。こちらは数が少ない（本番の依存は6件）ので自前で集める。
//
//   node ui/scripts/licenses.mjs >> THIRD-PARTY-LICENSES.txt
//
// **devDependencies は辿らない**。TypeScript や Vite はビルドに使うだけで
// 配布物には入らないため、載せると「配っていないものを配っていると書く」ことになる。
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const uiDir = join(dirname(fileURLToPath(import.meta.url)), "..");

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

/**
 * 本番の依存を推移的に辿る。
 *
 * npm は基本的に `node_modules` の直下へ巻き上げる（フラット配置）が、
 * **版が衝突したものは親の下に入れ子で置かれる**（`a/node_modules/b`）。
 * 直下だけを見ると、その依存を取りこぼすか、別の版の条文を載せてしまう。
 * ここは配布物のライセンス一覧を作る場所なので、取りこぼしは
 * 「載せるべき表示を載せていない」ことになる。親から順に上へ辿る。
 */
function resolvePkgDir(name, fromDir) {
  // Node の解決規則と同じ順序: 近い node_modules から外側へ
  let dir = fromDir;
  for (;;) {
    const candidate = join(dir, "node_modules", name);
    if (existsSync(join(candidate, "package.json"))) return candidate;
    const parent = dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

function collect(name, fromDir, into) {
  const dir = resolvePkgDir(name, fromDir);
  // 入っていないものは飛ばす（optionalDependencies が外れている場合など）
  if (!dir) return;
  if (into.has(dir)) return;
  const pkg = readJson(join(dir, "package.json"));
  into.set(dir, pkg);
  for (const dep of Object.keys(pkg.dependencies ?? {})) collect(dep, dir, into);
}

const root = readJson(join(uiDir, "package.json"));
const found = new Map();
for (const dep of Object.keys(root.dependencies ?? {})) collect(dep, uiDir, found);

const out = [];
out.push("");
out.push("=".repeat(80));
out.push("UI（JavaScript）側の依存");
out.push("=".repeat(80));
out.push("");

const sorted = [...found].sort(([, a], [, b]) => a.name.localeCompare(b.name));
for (const [dir, pkg] of sorted) {
  const name = pkg.name;
  const files = readdirSync(dir).filter((f) => /^licen[cs]e/i.test(f));
  const repo =
    typeof pkg.repository === "string" ? pkg.repository : (pkg.repository?.url ?? "");

  out.push("-".repeat(80));
  out.push(`${name} ${pkg.version}`);
  out.push(`ライセンス: ${pkg.license ?? "（package.json に記載なし）"}`);
  if (repo) out.push(`配布元: ${repo.replace(/^git\+/, "").replace(/\.git$/, "")}`);
  out.push("");
  if (files.length === 0) {
    // 条文が同梱されていない場合は、それが分かる形で残す。黙って空にしない
    out.push("（パッケージにライセンス条文が同梱されていない）");
    out.push("");
    continue;
  }
  for (const file of files) {
    if (files.length > 1) out.push(`[${file}]`);
    out.push(readFileSync(join(dir, file), "utf8").trimEnd());
    out.push("");
  }
}

out.push("-".repeat(80));
out.push("以上");
process.stdout.write(out.join("\n") + "\n");
