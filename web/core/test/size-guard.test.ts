import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { YSyncConfig, ySyncAnnotation } from "y-codemirror.next";
import * as Y from "yjs";
import { MAX_DOC_CHARS, sizeGuard } from "../src/editor";

function setup(doc = "") {
  let tooLarge = 0;
  const state = EditorState.create({ doc, extensions: sizeGuard(10, () => tooLarge++) });
  return { state, count: () => tooLarge };
}

describe("sizeGuard", () => {
  it("allows local edits up to the limit", () => {
    const { state, count } = setup("12345");
    const next = state.update({ changes: { from: 5, insert: "67890" } }).state;
    expect(next.doc.toString()).toBe("1234567890");
    expect(count()).toBe(0);
  });

  it("rejects a local edit past the limit and reports it", () => {
    const { state, count } = setup("12345");
    const next = state.update({ changes: { from: 5, insert: "678901" } }).state;
    expect(next.doc.toString()).toBe("12345");
    expect(count()).toBe(1);
  });

  it("never filters remote (y-sync) transactions", () => {
    const { state, count } = setup("12345");
    const ytext = new Y.Doc().getText("code");
    const remote = ySyncAnnotation.of(new YSyncConfig(ytext, null));
    const next = state.update({ changes: { from: 5, insert: "x".repeat(20) }, annotations: remote }).state;
    expect(next.doc.length).toBe(25);
    expect(count()).toBe(0);
  });

  it("allows edits that do not grow an oversized document", () => {
    const { state, count } = setup("x".repeat(20));
    const shrunk = state.update({ changes: { from: 0, to: 5 } }).state;
    expect(shrunk.doc.length).toBe(15);
    const replaced = state.update({ changes: { from: 0, to: 1, insert: "y" } }).state;
    expect(replaced.doc.length).toBe(20);
    const grown = state.update({ changes: { from: 0, insert: "y" } }).state;
    expect(grown.doc.length).toBe(20);
    expect(count()).toBe(1);
  });

  it("stays under the server's 1 MiB document cap", () => {
    expect(MAX_DOC_CHARS).toBe(900_000);
    expect(MAX_DOC_CHARS).toBeLessThan(1 << 20);
  });
});
