import type { Extension } from "@codemirror/state";
import { StreamLanguage } from "@codemirror/language";

export type LanguageId =
  | "javascript" | "typescript" | "python" | "go" | "rust" | "java" | "c" | "cpp"
  | "sql" | "json" | "geojson" | "css" | "html" | "swift" | "kotlin" | "objectivec"
  | "bash" | "protobuf" | "plaintext";

export interface LanguageDef {
  id: LanguageId;
  label: string;
  ext: string;
  load: () => Promise<Extension>;
}

export const DEFAULT_LANGUAGE: LanguageId = "python";

export const LANGUAGES: readonly LanguageDef[] = [
  { id: "javascript", label: "JavaScript", ext: "js", load: async () => (await import("@codemirror/lang-javascript")).javascript() },
  { id: "typescript", label: "TypeScript", ext: "ts", load: async () => (await import("@codemirror/lang-javascript")).javascript({ typescript: true }) },
  { id: "python", label: "Python", ext: "py", load: async () => (await import("@codemirror/lang-python")).python() },
  { id: "go", label: "Go", ext: "go", load: async () => (await import("@codemirror/lang-go")).go() },
  { id: "rust", label: "Rust", ext: "rs", load: async () => (await import("@codemirror/lang-rust")).rust() },
  { id: "java", label: "Java", ext: "java", load: async () => (await import("@codemirror/lang-java")).java() },
  { id: "c", label: "C", ext: "c", load: async () => (await import("@codemirror/lang-cpp")).cpp() },
  { id: "cpp", label: "C++", ext: "cpp", load: async () => (await import("@codemirror/lang-cpp")).cpp() },
  { id: "sql", label: "SQL", ext: "sql", load: async () => (await import("@codemirror/lang-sql")).sql() },
  { id: "json", label: "JSON", ext: "json", load: async () => (await import("@codemirror/lang-json")).json() },
  { id: "geojson", label: "GeoJSON", ext: "geojson", load: async () => (await import("@codemirror/lang-json")).json() },
  { id: "css", label: "CSS", ext: "css", load: async () => (await import("@codemirror/lang-css")).css() },
  { id: "html", label: "HTML", ext: "html", load: async () => (await import("@codemirror/lang-html")).html() },
  { id: "swift", label: "Swift", ext: "swift", load: async () => StreamLanguage.define((await import("@codemirror/legacy-modes/mode/swift")).swift) },
  { id: "kotlin", label: "Kotlin", ext: "kt", load: async () => StreamLanguage.define((await import("@codemirror/legacy-modes/mode/clike")).kotlin) },
  { id: "objectivec", label: "Objective-C", ext: "m", load: async () => StreamLanguage.define((await import("@codemirror/legacy-modes/mode/clike")).objectiveC) },
  { id: "bash", label: "Bash", ext: "sh", load: async () => StreamLanguage.define((await import("@codemirror/legacy-modes/mode/shell")).shell) },
  { id: "protobuf", label: "Protobuf", ext: "proto", load: async () => StreamLanguage.define((await import("@codemirror/legacy-modes/mode/protobuf")).protobuf) },
  { id: "plaintext", label: "Plain text", ext: "txt", load: async () => [] },
];

const PLAINTEXT = LANGUAGES[LANGUAGES.length - 1]!;

export function languageById(id: unknown): LanguageDef {
  return LANGUAGES.find((l) => l.id === id) ?? PLAINTEXT;
}
