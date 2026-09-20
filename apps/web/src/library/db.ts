// IndexedDB for the library: thumbnails, metadata and analysis cached by
// file key (path + size + modified time), marks per folder, folder handles,
// and action sets. Works on the UI thread and inside the thumbnail worker.

const NAME = "ops-library";
const VERSION = 1;
export const STORES = ["thumbs", "analysis", "marks", "folders", "actions", "settings"] as const;
export type StoreName = (typeof STORES)[number];

let opening: Promise<IDBDatabase> | null = null;

function open(): Promise<IDBDatabase> {
  opening ??= new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") return reject(new Error("IndexedDB is not available"));
    const req = indexedDB.open(NAME, VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      for (const s of STORES) if (!db.objectStoreNames.contains(s)) db.createObjectStore(s);
    };
    req.onsuccess = () => {
      const db = req.result;
      db.onversionchange = () => {
        db.close();
        opening = null;
      };
      resolve(db);
    };
    req.onerror = () => {
      opening = null;
      reject(req.error ?? new Error("could not open the library database"));
    };
  });
  return opening;
}

function wrap<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function get<T>(store: StoreName, key: IDBValidKey): Promise<T | undefined> {
  try {
    const db = await open();
    return (await wrap(db.transaction(store, "readonly").objectStore(store).get(key))) as T | undefined;
  } catch {
    return undefined;
  }
}

export async function put(store: StoreName, key: IDBValidKey, value: unknown): Promise<void> {
  try {
    const db = await open();
    await wrap(db.transaction(store, "readwrite").objectStore(store).put(value, key));
  } catch (e) {
    // Quota or private mode: the cache is an optimisation, never a failure.
    console.warn("library cache write failed", e);
  }
}

export async function del(store: StoreName, key: IDBValidKey): Promise<void> {
  try {
    const db = await open();
    await wrap(db.transaction(store, "readwrite").objectStore(store).delete(key));
  } catch {
    /* ignore */
  }
}

export async function getMany<T>(store: StoreName, keys: IDBValidKey[]): Promise<(T | undefined)[]> {
  if (!keys.length) return [];
  try {
    const db = await open();
    const os = db.transaction(store, "readonly").objectStore(store);
    return await Promise.all(keys.map((k) => wrap(os.get(k)) as Promise<T | undefined>));
  } catch {
    return keys.map(() => undefined);
  }
}

export async function all<T>(store: StoreName): Promise<{ key: IDBValidKey; value: T }[]> {
  try {
    const db = await open();
    const os = db.transaction(store, "readonly").objectStore(store);
    const [keys, values] = await Promise.all([wrap(os.getAllKeys()), wrap(os.getAll())]);
    return keys.map((key, i) => ({ key, value: values[i] as T }));
  } catch {
    return [];
  }
}
