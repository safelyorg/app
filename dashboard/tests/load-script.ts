// The dashboard's ts/*.ts files are plain browser scripts: their
// functions are globals that the other files call. A test loads the
// real file the same way - turned into JavaScript, then run on the
// page - so its functions can be called directly.
//
// Node's file tools and Vite are loaded by name, so these helpers need
// no Node types in the editor.
const NODE_FS = "node:fs";
const NODE_PATH = "node:path";
const VITE = "vite";

// A file's text. The path is from the dashboard folder, where the
// tests run ("index.html", "ts/settings.ts").
export async function readText(file: string): Promise<string> {
  const fs: any = await import(/* @vite-ignore */ NODE_FS);
  const path: any = await import(/* @vite-ignore */ NODE_PATH);
  const root: string = (globalThis as any).process.cwd();
  return fs.readFileSync(path.resolve(root, file), "utf8");
}

const NODE_MODULE = "node:module";

// TypeScript to plain JavaScript. The TypeScript package itself isn't
// used: TypeScript 7 has no JavaScript API for this. Instead:
// 1. Node's own stripTypeScriptTypes (Node 22.13 and newer), or
// 2. Vite's transformWithOxc (Vite 8, which comes with vitest 5).
async function toJavaScript(source: string, file: string): Promise<string> {
  const nodeModule: any = await import(/* @vite-ignore */ NODE_MODULE);
  if (typeof nodeModule.stripTypeScriptTypes === "function") {
    return nodeModule.stripTypeScriptTypes(source);
  }
  const vite: any = await import(/* @vite-ignore */ VITE);
  if (typeof vite.transformWithOxc === "function") {
    return (await vite.transformWithOxc(source, file, { lang: "ts" })).code;
  }
  throw new Error("Update Node (22.13 or newer) or vitest (5 or newer) to run these tests.");
}

export async function loadScript(file: string): Promise<void> {
  const js = await toJavaScript(await readText(file), file);
  // Run as a page script (not a module), so its functions become globals.
  (0, eval)(js);
}

export function global<T = any>(name: string): T {
  return (globalThis as any)[name];
}
