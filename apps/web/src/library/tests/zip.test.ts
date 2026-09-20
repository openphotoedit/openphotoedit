/// <reference types="node" />
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { crc32, ZipWriter } from "../zip";

const OUT = resolve(__dirname, "../../../../../target/library/zip-test");

describe("store-only ZIP writer", () => {
  it("computes the standard CRC-32", () => {
    expect(crc32(new TextEncoder().encode("123456789"))).toBe(0xcbf43926);
    expect(crc32(new Uint8Array(0))).toBe(0);
  });

  it("writes an archive unzip accepts, with byte-identical entries", async () => {
    mkdirSync(OUT, { recursive: true });
    const photo = new Uint8Array(readFileSync(resolve(__dirname, "../../../../../testdata/photos/portrait.jpg")));
    const z = new ZipWriter();
    await z.add("Web 2048/portrait.jpg", new Blob([photo]));
    await z.add("Instagram/Ünïcode name – 1080×1350.jpg", photo.subarray(0, 5000));
    await z.add("empty.txt", new Uint8Array(0));
    expect(z.has("empty.txt")).toBe(true);
    await expect(z.add("empty.txt", new Uint8Array(1))).rejects.toThrow(/duplicate/);
    const blob = z.finish();
    const path = join(OUT, "test.zip");
    writeFileSync(path, new Uint8Array(await blob.arrayBuffer()));
    const test = execFileSync("unzip", ["-t", path], { encoding: "utf8" });
    expect(test).toMatch(/No errors detected/);
    expect(test.match(/testing:/g)?.length).toBe(3);
    const back = execFileSync("unzip", ["-p", path, "Web 2048/portrait.jpg"], { maxBuffer: 64 << 20 });
    expect(Buffer.compare(back, Buffer.from(photo))).toBe(0);
    // Python's zipfile is a second, stricter reader.
    const py = execFileSync("python3", ["-c", "import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); import json; print(z.testzip(), json.dumps([[i.filename, i.file_size] for i in z.infolist()], ensure_ascii=False))", path], { encoding: "utf8" });
    expect(py.trim()).toBe(`None ${JSON.stringify([["Web 2048/portrait.jpg", photo.length], ["Instagram/Ünïcode name – 1080×1350.jpg", 5000], ["empty.txt", 0]]).replace(/,/g, ", ")}`);
  });
});
