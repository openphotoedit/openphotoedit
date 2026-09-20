// A store-only ZIP writer with no dependencies. Entries are kept as Blobs
// and the archive is a Blob of slices, so nothing is copied into one big
// buffer: a 2 GB export costs the output files' memory, not twice that.
// Limits: no ZIP64, so under 4 GiB total and 65,535 entries.

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

export function crc32(data: Uint8Array, crc = 0): number {
  let c = ~crc >>> 0;
  for (let i = 0; i < data.length; i++) c = CRC_TABLE[(c ^ data[i]) & 0xff] ^ (c >>> 8);
  return ~c >>> 0;
}

function dosTime(d: Date) {
  const time = (d.getHours() << 11) | (d.getMinutes() << 5) | (Math.floor(d.getSeconds() / 2) & 0x1f);
  const date = ((Math.max(1980, d.getFullYear()) - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate();
  return { time, date };
}

interface Entry {
  name: Uint8Array;
  crc: number;
  size: number;
  offset: number;
  time: number;
  date: number;
}

export class ZipWriter {
  private parts: BlobPart[] = [];
  private entries: Entry[] = [];
  private offset = 0;
  private names = new Set<string>();

  get count() {
    return this.entries.length;
  }

  has(name: string) {
    return this.names.has(name);
  }

  async add(name: string, data: Blob | Uint8Array, modified = new Date()) {
    const clean = name.replace(/\\/g, "/").replace(/^\/+/, "");
    if (this.names.has(clean)) throw new Error(`duplicate entry ${clean}`);
    const bytes = data instanceof Uint8Array ? data : new Uint8Array(await data.arrayBuffer());
    const nameBytes = new TextEncoder().encode(clean);
    const crc = crc32(bytes);
    const { time, date } = dosTime(modified);
    if (this.offset + 30 + nameBytes.length + bytes.length > 0xffffffff || this.entries.length >= 0xffff) {
      throw new Error("the archive is too large for a ZIP without ZIP64; export to a folder instead");
    }
    const h = new DataView(new ArrayBuffer(30));
    h.setUint32(0, 0x04034b50, true);
    h.setUint16(4, 20, true); // version needed
    h.setUint16(6, 0x0800, true); // UTF-8 names
    h.setUint16(8, 0, true); // stored
    h.setUint16(10, time, true);
    h.setUint16(12, date, true);
    h.setUint32(14, crc, true);
    h.setUint32(18, bytes.length, true);
    h.setUint32(22, bytes.length, true);
    h.setUint16(26, nameBytes.length, true);
    h.setUint16(28, 0, true);
    this.parts.push(h.buffer, nameBytes as BlobPart, data instanceof Blob ? data : (bytes as BlobPart));
    this.entries.push({ name: nameBytes, crc, size: bytes.length, offset: this.offset, time, date });
    this.names.add(clean);
    this.offset += 30 + nameBytes.length + bytes.length;
  }

  finish(): Blob {
    const cdStart = this.offset;
    const cd: BlobPart[] = [];
    let cdSize = 0;
    for (const e of this.entries) {
      const h = new DataView(new ArrayBuffer(46));
      h.setUint32(0, 0x02014b50, true);
      h.setUint16(4, 20, true); // made by
      h.setUint16(6, 20, true); // needed
      h.setUint16(8, 0x0800, true);
      h.setUint16(10, 0, true);
      h.setUint16(12, e.time, true);
      h.setUint16(14, e.date, true);
      h.setUint32(16, e.crc, true);
      h.setUint32(20, e.size, true);
      h.setUint32(24, e.size, true);
      h.setUint16(28, e.name.length, true);
      h.setUint16(30, 0, true);
      h.setUint16(32, 0, true);
      h.setUint16(34, 0, true);
      h.setUint16(36, 0, true);
      h.setUint32(38, 0, true);
      h.setUint32(42, e.offset, true);
      cd.push(h.buffer, e.name as BlobPart);
      cdSize += 46 + e.name.length;
    }
    const end = new DataView(new ArrayBuffer(22));
    end.setUint32(0, 0x06054b50, true);
    end.setUint16(8, this.entries.length, true);
    end.setUint16(10, this.entries.length, true);
    end.setUint32(12, cdSize, true);
    end.setUint32(16, cdStart, true);
    return new Blob([...this.parts, ...cd, end.buffer], { type: "application/zip" });
  }
}
