import { describe, expect, it } from "vitest";
import { DEFAULT_LANGUAGE, LANGUAGES, languageById } from "../src/index";

const ORDER = [
  "javascript", "typescript", "python", "go", "rust", "java", "c", "cpp", "sql",
  "json", "geojson", "css", "html", "swift", "kotlin", "objectivec", "bash",
  "protobuf", "plaintext",
];

describe("languages", () => {
  it("every language loads", async () => {
    for (const lang of LANGUAGES) {
      const ext = await lang.load();
      expect(typeof ext).toBe("object");
    }
  });

  it("ids are unique and match the union order", () => {
    const ids = LANGUAGES.map((l) => l.id);
    expect(ids).toEqual(ORDER);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("geojson uses json highlighting and .geojson ext", () => {
    expect(languageById("geojson").ext).toBe("geojson");
    expect(languageById("json").ext).toBe("json");
  });

  it("falls back to plaintext for unknown ids", () => {
    expect(languageById("nope").id).toBe("plaintext");
    expect(languageById(undefined).id).toBe("plaintext");
  });

  it("has the expected default and extensions", () => {
    expect(DEFAULT_LANGUAGE).toBe("python");
    expect(LANGUAGES.map((l) => l.ext)).toEqual([
      "js", "ts", "py", "go", "rs", "java", "c", "cpp", "sql", "json", "geojson",
      "css", "html", "swift", "kt", "m", "sh", "proto", "txt",
    ]);
  });
});
