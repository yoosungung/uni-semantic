import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const schemaPath = path.join(root, "document.schema.json");
const validDir = path.join(root, "fixtures", "valid");
const invalidDir = path.join(root, "fixtures", "invalid");

async function loadJson(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}

async function listJson(dir) {
  const names = await readdir(dir);
  return names
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map((name) => path.join(dir, name));
}

test("document.schema.json loads and compiles", async () => {
  const schema = await loadJson(schemaPath);
  assert.equal(
    schema.$id,
    "https://uni-semantic.local/ir/document.schema.json",
  );
  assert.match(schema.description ?? "", /sql-mcp/);
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  assert.equal(typeof validate, "function");
});

test("valid fixtures pass schema", async () => {
  const schema = await loadJson(schemaPath);
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  const files = await listJson(validDir);
  assert.ok(files.length >= 5, "expected at least 5 valid fixtures");
  for (const file of files) {
    const doc = await loadJson(file);
    const ok = validate(doc);
    assert.equal(
      ok,
      true,
      `${path.basename(file)} should be valid: ${ajv.errorsText(validate.errors)}`,
    );
  }
});

test("invalid fixtures fail schema", async () => {
  const schema = await loadJson(schemaPath);
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  const files = await listJson(invalidDir);
  assert.ok(files.length >= 2, "expected at least 2 invalid fixtures");
  for (const file of files) {
    const doc = await loadJson(file);
    const ok = validate(doc);
    assert.equal(
      ok,
      false,
      `${path.basename(file)} should be invalid`,
    );
  }
});

test("schema $defs include §3 extension identifiers", async () => {
  const schema = await loadJson(schemaPath);
  for (const key of [
    "dialect",
    "unresolved_expression",
    "hierarchy",
    "role_playing",
  ]) {
    assert.ok(schema.$defs?.[key], `missing $defs.${key}`);
  }
});
