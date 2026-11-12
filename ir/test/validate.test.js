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
    "snowflake_path",
  ]) {
    assert.ok(schema.$defs?.[key], `missing $defs.${key}`);
  }
});

test("Phase 2 snowflake_path and hierarchy fixtures cover schema contracts", async () => {
  const schema = await loadJson(schemaPath);
  const pathDef = schema.$defs?.snowflake_path;
  assert.equal(pathDef?.type, "array");
  assert.equal(pathDef?.minItems, 1);
  assert.equal(pathDef?.uniqueItems, true);
  assert.equal(
    schema.$defs?.viewJoin?.properties?.snowflake_path?.$ref,
    "#/$defs/snowflake_path",
  );

  const hierarchyDef = schema.$defs?.hierarchy;
  assert.equal(hierarchyDef?.properties?.levels?.minItems, 2);
  assert.equal(hierarchyDef?.properties?.levels?.uniqueItems, true);
  assert.ok(
    hierarchyDef?.properties?.description,
    "hierarchy.description should be optional additive field",
  );

  const validDocs = await Promise.all(
    (await listJson(validDir)).map(async (file) => ({
      file: path.basename(file),
      doc: await loadJson(file),
    })),
  );

  const snowflakeJoin = validDocs
    .filter(({ doc }) => doc.kind === "view")
    .flatMap(({ doc, file }) =>
      (doc.joins ?? []).map((join) => ({ file, join })),
    )
    .find(({ join }) => Array.isArray(join.snowflake_path));
  assert.ok(snowflakeJoin, "expected ≥1 valid view join with snowflake_path");
  assert.ok(
    snowflakeJoin.join.snowflake_path.length >= 1,
    `${snowflakeJoin.file}: snowflake_path minItems 1`,
  );

  const hierarchyCube = validDocs.find(
    ({ doc }) =>
      doc.kind === "cube" &&
      Array.isArray(doc.hierarchies) &&
      doc.hierarchies.some(
        (h) => Array.isArray(h.levels) && h.levels.length >= 3,
      ),
  );
  assert.ok(
    hierarchyCube,
    "expected ≥1 valid cube hierarchy with ≥3 levels",
  );

  const invalidNames = (await listJson(invalidDir)).map((f) =>
    path.basename(f),
  );
  assert.ok(
    invalidNames.includes("snowflake-empty-path.json"),
    "expected invalid snowflake-empty-path.json",
  );
  assert.ok(
    invalidNames.includes("hierarchy-dup-levels.json"),
    "expected invalid hierarchy-dup-levels.json",
  );
});

test("Phase 1 basic-element fixtures cover pk/fk, inner|left+relationship, sum|count|avg", async () => {
  const files = await listJson(validDir);
  const docs = await Promise.all(files.map(async (file) => ({
    file: path.basename(file),
    doc: await loadJson(file),
  })));

  const tables = docs.filter(({ doc }) => doc.kind === "table");
  assert.ok(
    tables.some(
      ({ doc }) =>
        Array.isArray(doc.pk) &&
        doc.pk.length >= 1 &&
        Array.isArray(doc.fk) &&
        doc.fk.length >= 1,
    ),
    "expected ≥1 table fixture with nonempty pk and fk",
  );

  const joinTypes = new Set();
  for (const { doc } of docs.filter(({ doc }) => doc.kind === "view")) {
    for (const join of doc.joins ?? []) {
      if (
        (join.type === "inner" || join.type === "left") &&
        typeof join.relationship === "string"
      ) {
        joinTypes.add(join.type);
      }
    }
  }
  assert.ok(joinTypes.has("inner"), "expected inner join fixture with relationship");
  assert.ok(joinTypes.has("left"), "expected left join fixture with relationship");

  const measureFns = new Set();
  for (const { doc } of docs.filter(({ doc }) => doc.kind === "metric")) {
    const m = String(doc.expression ?? "").match(/^\s*(sum|count|avg)\s*\(/i);
    if (m) measureFns.add(m[1].toLowerCase());
    assert.ok(
      ["group", "rollup", "cube", "grouping_sets"].includes(doc.aggregation),
      `${doc.name}: aggregation enum is GROUPING axis, not measure fn`,
    );
  }
  for (const fn of ["sum", "count", "avg"]) {
    assert.ok(measureFns.has(fn), `expected ≥1 metric.expression using ${fn}(...)`);
  }
});
