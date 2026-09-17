// The model catalogue, and the cache models live in after first use.
//
// Served from this app's own origin (`./models/`), never from upstream:
// a cross-origin request timed to the moment someone opens a private photo
// would undercut "nothing leaves your device" even though the photo does
// not move. Cache Storage makes the second visit need no network at all.
//
// Every file is pinned by size and sha256 (verified with WebCrypto before it
// is cached, so a truncated or substituted file is never kept). Provenance
// and licences: MODELS.md. `scripts/fetch-models.sh` puts the files in place.
//
// Runs on both threads: the worker downloads and runs models; the UI lists,
// sizes and forgets them.

export type Backend = "webgpu" | "wasm";
export type Tier = 1 | 2;

export interface ModelFile {
  /** Path under `models/`. The first file is the graph; later files are its external data. */
  path: string;
  bytes: number;
  sha256: string;
  /** Where scripts/fetch-models.sh and the native server download it from. */
  url: string;
  /** Not downloadable as ONNX: `url` is the PyTorch source, converted by openpixels/scripts/export-models.py. */
  exported?: boolean;
  upstream?: string;
}

export interface ModelSpec {
  id: string;
  label: string;
  /** What it does, for the Models page. */
  feature: string;
  files: ModelFile[];
  licence: string;
  attribution: string;
  source: string;
  /** 1 = small default, 2 = larger optional download. */
  tier: Tier;
  /** Backends measured to give wrong output for this exact file. */
  badBackends?: Backend[];
  /** A licence or dataset caveat recorded for review (research 04 §5). */
  greyZone?: string;
  /** Canary signature recorded on the CPU backend (see jobs/canary.ts). */
  canary?: number[];
}

const MB = 1024 * 1024;

export const MODELS: Record<string, ModelSpec> = {
  migan: {
    id: "migan",
    label: "Remove (fast)",
    feature: "Removes selected objects and fills the gap",
    files: [{ path: "migan_pipeline_v2.onnx", bytes: 28_079_181, sha256: "6f1f3530a1a2324b19752018ce756088b07973cda8d7d890034ace5c8a48c40b", url: "https://huggingface.co/andraniksargsyan/migan/resolve/main/migan_pipeline_v2.onnx" }],
    licence: "MIT (code and weights)",
    attribution: "MI-GAN, Picsart AI Research (Sargsyan et al., ICCV 2023)",
    source: "https://huggingface.co/andraniksargsyan/migan",
    tier: 1,
    canary: [171.416, 178.037, 182.707, 158.853, 153.567, 181.13, 161.396, 135.888, 98.694, 122.503, 82.1194, 76.868, 29.8314, 44.5778, 26.071, 19.3752, 102.618, 158.441, 175.966, 75.5518, 86.6559, 136.786, 118.702, 59.2632, 63.1189, 125.963, 86.1394, 49.2758, 39.6251, 67.831, 32.9719, 25.4489, 93.01, 140.807, 165.516, 67.2207, 76.0958, 110.58, 95.6033, 49.5143, 63.0493, 135.433, 95.8448, 54.3491, 54.8301, 107.693, 46.3577, 37.5175],
  },
  lama: {
    id: "lama",
    label: "Remove (best)",
    feature: "Removes larger objects with more coherent structure",
    files: [{ path: "lama_fp32.onnx", bytes: 208_044_816, sha256: "1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6", url: "https://huggingface.co/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx" }],
    licence: "Apache-2.0",
    attribution: "LaMa (Suvorov et al., WACV 2022), ONNX port by Carve",
    source: "https://huggingface.co/Carve/LaMa-ONNX",
    tier: 2,
    greyZone: "Trained on Places2, whose image terms are unverified",
    canary: [171.416, 178.037, 182.707, 158.853, 153.567, 178.414, 161.795, 135.888, 98.6949, 118.475, 82.5345, 76.868, 29.833, 44.5787, 26.0711, 19.3753, 102.619, 158.441, 175.966, 75.5519, 86.6562, 134.805, 119.57, 59.2632, 63.1189, 122.384, 87.1875, 49.2758, 39.6321, 67.8417, 32.972, 25.449, 93.0176, 140.807, 165.516, 67.2207, 76.1005, 110.192, 96.3124, 49.5143, 63.0497, 133.284, 96.1915, 54.3491, 54.8301, 107.693, 46.3577, 37.5175],
  },
  modnet: {
    id: "modnet",
    label: "Portrait subject",
    feature: "Finds people for select subject, remove and blur background",
    files: [{ path: "modnet_photographic_portrait_matting.onnx", bytes: 25_888_640, sha256: "07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9", url: "https://github.com/Zeyi-Lin/HivisionIDPhotos/releases/download/pretrained-model/modnet_photographic_portrait_matting.onnx" }],
    licence: "Apache-2.0",
    attribution: "MODNet (Ke et al., AAAI 2022), weights via HivisionIDPhotos",
    source: "https://github.com/Zeyi-Lin/HivisionIDPhotos/releases/tag/pretrained-model",
    tier: 1,
    // OpenPhotoId, 8 Sep 2026: onnxruntime-web's WebGPU provider returned a
    // matte full of holes through the hair while the CPU path was exact.
    badBackends: ["webgpu"],
    canary: [0.0011, 0.6392, 0.5639, 0.0001, 0.0295, 0.8672, 0.8011, 0.0015, 0.391, 0.8796, 0.913, 0.5395, 0.9997, 0.9998, 0.9998, 0.9998],
  },
  u2netp: {
    id: "u2netp",
    label: "Object subject",
    feature: "Finds the main object when there is no person",
    files: [{ path: "u2netp.onnx", bytes: 4_574_861, sha256: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8", url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx" }],
    licence: "Apache-2.0",
    attribution: "U²-Net (Qin et al., 2020), rembg release",
    source: "https://github.com/danielgatis/rembg/releases/tag/v0.0.0",
    tier: 1,
    canary: [0.0007, 0.4697, 0.4226, 0.0002, 0.0196, 0.853, 0.7338, 0.0005, 0.3918, 0.8861, 0.9212, 0.5501, 0.9993, 0.9998, 0.9999, 0.9973],
  },
  "edgetam-encoder": {
    id: "edgetam-encoder",
    label: "Object selection (image)",
    feature: "Reads the photo once so clicks select objects instantly",
    files: [
      { path: "edgetam/vision_encoder.onnx", bytes: 192_225, sha256: "ed068218eba96760fe02d04ce899c449660ac813a088d80ed7f42c8bb01e7cec", url: "https://huggingface.co/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/vision_encoder.onnx" },
      { path: "edgetam/vision_encoder.onnx_data", bytes: 19_532_576, sha256: "21e75dba7077dfcb53e8c9a6e99977156f2240ff3f1f9cddc43d66aa1ecb528e", url: "https://huggingface.co/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/vision_encoder.onnx_data" },
    ],
    licence: "Apache-2.0",
    attribution: "EdgeTAM, Meta (Zhou et al., CVPR 2025), ONNX by onnx-community",
    source: "https://huggingface.co/onnx-community/EdgeTAM-ONNX",
    tier: 1,
    canary: [0.1494, 0.066, 0.0659, 0.2336, 0.1164, 0.065, 0.0403, 0.1917, 0.0965, 0.0648, 0.0917, 0.1262, 0.0603, 0.0258, 0.0494, 0.0175, -0.0597, -0.2624, -0.193, -0.0465, -0.0784, -0.1884, -0.1419, -0.0631, -0.1481, -0.1921, -0.178, -0.0793, -0.1849, -0.1059, -0.1551, -0.1252, -0.0847, -0.1061, -0.2482, 0.197, -0.1333, 0.1254, 0.1305, 0.1307, -0.1547, 0.0389, 0.0608, 0.0217, -0.1802, -0.0736, -0.0555, -0.0878],
  },
  "edgetam-decoder": {
    id: "edgetam-decoder",
    label: "Object selection (clicks)",
    feature: "Turns clicks into an object mask",
    files: [
      { path: "edgetam/prompt_encoder_mask_decoder.onnx", bytes: 213_114, sha256: "d3668299ec3edf70fbb139ec642b54bf3d4be453fd1b688a6b5938e0856fe546", url: "https://huggingface.co/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/prompt_encoder_mask_decoder.onnx" },
      { path: "edgetam/prompt_encoder_mask_decoder.onnx_data", bytes: 20_958_208, sha256: "dfa2125e30d08d388732f20c18fb63ab0f0f590cd270eb307f0c2b65919d1be8", url: "https://huggingface.co/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/prompt_encoder_mask_decoder.onnx_data" },
    ],
    licence: "Apache-2.0",
    attribution: "EdgeTAM, Meta (Zhou et al., CVPR 2025), ONNX by onnx-community",
    source: "https://huggingface.co/onnx-community/EdgeTAM-ONNX",
    tier: 1,
  },
  yunet: {
    id: "yunet",
    label: "Face detection",
    feature: "Finds faces for face restoration, red eye and scene analysis",
    files: [{ path: "face_detection_yunet_2023mar.onnx", bytes: 232_589, sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4", url: "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx" }],
    licence: "MIT",
    attribution: "YuNet (Wu et al.), OpenCV Zoo",
    source: "https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet",
    tier: 1,
    canary: [0.548, 0.7487, 0.689, 0.4802, 0.5225, 0.8177, 0.8249, 0.5374, 0.509, 0.6511, 0.678, 0.5778, 0.5586, 0.6597, 0.6312, 0.5556],
  },
  "realesr-x4v3": {
    id: "realesr-x4v3",
    label: "Upscale (fast)",
    feature: "Upscales photos 2× or 4×; also cleans JPEG artifacts at 1×",
    files: [{ path: "realesr_general_x4v3.onnx", bytes: 4_866_422, sha256: "0e121299bf9b23a764d3f9e2120d0d0727d5ac5032e03a46cb78d28bcc7c6872", url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-x4v3.pth", upstream: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-x4v3.pth", exported: true }],
    licence: "BSD-3-Clause",
    attribution: "Real-ESRGAN (Wang et al., 2021), exported by OpenPixels",
    source: "https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0",
    tier: 1,
    canary: [0.6697, 0.7025, 0.7234, 0.6191, 0.5984, 0.7084, 0.6296, 0.5321, 0.3851, 0.4638, 0.3182, 0.3006, 0.1174, 0.1631, 0.1016, 0.072, 0.3964, 0.6244, 0.6959, 0.2868, 0.3313, 0.5337, 0.4653, 0.2265, 0.2446, 0.4755, 0.3332, 0.1911, 0.1567, 0.2566, 0.1297, 0.0983, 0.3584, 0.5555, 0.6581, 0.2523, 0.2895, 0.4361, 0.3752, 0.1858, 0.244, 0.5178, 0.3684, 0.2091, 0.2163, 0.4194, 0.181, 0.1446],
  },
  "realesr-x4v3-dn50": {
    id: "realesr-x4v3-dn50",
    label: "Denoise (medium)",
    feature: "Reduces noise",
    files: [{ path: "realesr_general_x4v3_dn50.onnx", bytes: 4_866_422, sha256: "b345bc59c38dbb78d871b5adf9519c475fe243c198f70cc9d0b97a464715d50b", url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-wdn-x4v3.pth", upstream: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-wdn-x4v3.pth", exported: true }],
    licence: "BSD-3-Clause",
    attribution: "Real-ESRGAN (Wang et al., 2021); 50/50 blend of the wdn and general weights, exported by OpenPixels",
    source: "https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0",
    tier: 1,
    canary: [0.6647, 0.6976, 0.7192, 0.6148, 0.5921, 0.7064, 0.6264, 0.5282, 0.3828, 0.4623, 0.3185, 0.299, 0.1185, 0.1659, 0.1028, 0.0725, 0.3958, 0.6224, 0.693, 0.2879, 0.3311, 0.5344, 0.4648, 0.2271, 0.2454, 0.476, 0.334, 0.1914, 0.1572, 0.2574, 0.1307, 0.0987, 0.357, 0.5544, 0.655, 0.2534, 0.2879, 0.4378, 0.3757, 0.1873, 0.2445, 0.5202, 0.37, 0.2104, 0.2173, 0.4224, 0.1829, 0.1463],
  },
  "realesr-x4v3-wdn": {
    id: "realesr-x4v3-wdn",
    label: "Denoise (strong)",
    feature: "Reduces heavy noise",
    files: [{ path: "realesr_general_wdn_x4v3.onnx", bytes: 4_866_422, sha256: "be536211515f088de05f1aa799a8079e92f49978b47aad6cec007f9fca62bb68", url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-wdn-x4v3.pth", upstream: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-wdn-x4v3.pth", exported: true }],
    licence: "BSD-3-Clause",
    attribution: "Real-ESRGAN (Wang et al., 2021), exported by OpenPixels",
    source: "https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0",
    tier: 1,
    canary: [0.6667, 0.6968, 0.719, 0.6177, 0.5934, 0.7063, 0.6264, 0.5298, 0.3827, 0.4618, 0.3185, 0.299, 0.1184, 0.1694, 0.103, 0.0725, 0.3952, 0.6213, 0.6914, 0.2878, 0.331, 0.5341, 0.4642, 0.2265, 0.2438, 0.4749, 0.3326, 0.19, 0.1553, 0.2577, 0.1291, 0.0965, 0.3616, 0.5567, 0.6567, 0.2589, 0.2941, 0.4417, 0.38, 0.1915, 0.2468, 0.5227, 0.3713, 0.2116, 0.2174, 0.4234, 0.184, 0.147],
  },
  "realesrgan-x4plus": {
    id: "realesrgan-x4plus",
    label: "Upscale (best)",
    feature: "Sharper upscaling for real photographs; slower",
    files: [{ path: "realesrgan_x4plus.onnx", bytes: 67_051_644, sha256: "800a80063abcc8db53f6579407347ac2d53a9f5697dcc839cff38fbd806faf37", url: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth", upstream: "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth", exported: true }],
    licence: "BSD-3-Clause",
    attribution: "Real-ESRGAN (Wang et al., 2021), exported by OpenPixels",
    source: "https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.1.0",
    tier: 2,
    canary: [0.6779, 0.7054, 0.7275, 0.6239, 0.6047, 0.7112, 0.6302, 0.5327, 0.3872, 0.4599, 0.3239, 0.3052, 0.1211, 0.1575, 0.1023, 0.0725, 0.3994, 0.6261, 0.7026, 0.2908, 0.3316, 0.5289, 0.4621, 0.2261, 0.2431, 0.4683, 0.337, 0.1917, 0.1567, 0.242, 0.1284, 0.0955, 0.3674, 0.564, 0.6773, 0.2667, 0.2991, 0.4355, 0.3767, 0.1965, 0.2466, 0.5127, 0.3742, 0.2113, 0.2123, 0.3994, 0.1782, 0.1418],
  },
  gfpgan: {
    id: "gfpgan",
    label: "Face restoration",
    feature: "Rebuilds blurred or damaged faces",
    files: [{ path: "gfpgan_1.4.onnx", bytes: 340_299_087, sha256: "accc4757b26bdb89b32b4d3500d4f79c9dff97c1dd7c7104bf9dcb95e3311385", url: "https://huggingface.co/facefusion/models-3.0.0/resolve/main/gfpgan_1.4.onnx" }],
    licence: "Apache-2.0",
    attribution: "GFPGAN v1.4 (Wang et al., CVPR 2021), ONNX via facefusion",
    source: "https://huggingface.co/facefusion/models-3.0.0",
    tier: 2,
    greyZone: "Trained on FFHQ (CC BY-NC-SA images); the repo licence says Apache-2.0 except third-party components",
    canary: [0.3179, 0.3662, 0.3952, 0.2389, 0.1575, 0.3969, 0.2502, 0.0694, -0.2274, -0.0716, -0.3482, -0.39, -0.7541, -0.6392, -0.7776, -0.8469, -0.1837, 0.2142, 0.3497, -0.3974, -0.3168, 0.0725, -0.0453, -0.5099, -0.483, -0.0341, -0.3126, -0.5922, -0.6807, -0.444, -0.7196, -0.7869, -0.2849, 0.0933, 0.2693, -0.4786, -0.4104, -0.0824, -0.1857, -0.6054, -0.4975, 0.0572, -0.2349, -0.5657, -0.5787, -0.1738, -0.6213, -0.681],
  },
  deoldify: {
    id: "deoldify",
    label: "Colorize",
    feature: "Adds colour to black-and-white photos",
    files: [{ path: "deoldify_artistic.onnx", bytes: 255_044_725, sha256: "9ac296cf05fecbdb604f50211f632b722402bc1f9a96ee5a8987b01c0c3c688f", url: "https://huggingface.co/facefusion/models-3.0.0/resolve/main/deoldify_artistic.onnx" }],
    licence: "MIT",
    attribution: "DeOldify (Jason Antic), artistic model, ONNX via facefusion",
    source: "https://huggingface.co/facefusion/models-3.0.0",
    tier: 2,
    canary: [118.821, 174.273, 180.368, 97.4016, 101.929, 168.492, 143.766, 79.3764, 70.8159, 122.952, 84.0903, 55.4244, 36.0315, 62.2891, 30.1129, 22.523, 123.495, 159.287, 176.809, 100.38, 107.141, 141.166, 125.132, 81.6605, 74.6734, 121.88, 85.1596, 58.3325, 38.4598, 66.0804, 32.5359, 24.7028, 123.33, 157.104, 176.545, 100.773, 105.22, 138.662, 123.812, 82.1954, 75.5491, 124.396, 87.8265, 62.194, 43.1109, 68.3918, 34.578, 28.8246],
  },
};

export function modelBytes(id: string): number {
  return MODELS[id]?.files.reduce((a, f) => a + f.bytes, 0) ?? 0;
}

export function formatSize(bytes: number): string {
  return bytes >= MB ? `${Math.round(bytes / MB)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

const CACHE = "openphotoedit-models-v1";

let base: string | null = null;

/** Where `models/` lives: the app's base URL (the worker is told it at init). */
export function setModelBase(baseUrl: string) {
  base = baseUrl;
}

function modelUrl(path: string): string {
  const b = base ?? (typeof document !== "undefined" ? document.baseURI : null);
  if (!b) throw new Error("model base URL is not configured");
  return new URL(`models/${path}`, b).href;
}

async function openCache(): Promise<Cache | null> {
  if (typeof caches === "undefined") return null;
  return caches.open(CACHE).catch(() => null);
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as unknown as ArrayBuffer);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export interface DownloadProgress {
  id: string;
  received: number;
  total: number;
  cached: boolean;
}

/** Fetch every file of a model, preferring the cache; verified before caching. */
export async function fetchModel(id: string, onProgress?: (p: DownloadProgress) => void): Promise<Uint8Array[]> {
  const spec = MODELS[id];
  if (!spec) throw new Error(`unknown model ${id}`);
  const total = modelBytes(id);
  const cache = await openCache();
  const out: Uint8Array[] = [];
  let done = 0;
  for (const file of spec.files) {
    const url = modelUrl(file.path);
    const hit = cache ? await cache.match(url) : undefined;
    if (hit) {
      const buf = new Uint8Array(await hit.arrayBuffer());
      if (buf.byteLength === file.bytes) {
        out.push(buf);
        done += file.bytes;
        onProgress?.({ id, received: done, total, cached: true });
        continue;
      }
      await cache!.delete(url);
    }
    const res = await fetch(url);
    if (!res.ok) throw new Error(`Could not download ${spec.label} (${res.status}). Run scripts/fetch-models.sh, or check the connection.`);
    const buf = new Uint8Array(file.bytes);
    let at = 0;
    if (res.body) {
      const reader = res.body.getReader();
      for (;;) {
        const { done: end, value } = await reader.read();
        if (end) break;
        if (at + value.length > buf.length) throw new Error(`${file.path} is larger than its pinned size`);
        buf.set(value, at);
        at += value.length;
        onProgress?.({ id, received: done + at, total, cached: false });
      }
    } else {
      const all = new Uint8Array(await res.arrayBuffer());
      buf.set(all.subarray(0, buf.length));
      at = all.length;
    }
    if (at !== file.bytes) throw new Error(`${file.path}: expected ${file.bytes} bytes, got ${at}`);
    const got = await sha256Hex(buf);
    if (got !== file.sha256) throw new Error(`${file.path} failed its checksum; it was not kept`);
    if (cache) await cache.put(url, new Response(buf, { headers: { "content-type": "application/octet-stream" } })).catch(() => {});
    out.push(buf);
    done += file.bytes;
  }
  return out;
}

/** Ids of models whose every file is on this device. */
export async function cachedModels(): Promise<string[]> {
  const cache = await openCache();
  if (!cache) return [];
  const out: string[] = [];
  for (const spec of Object.values(MODELS)) {
    let all = true;
    for (const f of spec.files) {
      if (!(await cache.match(modelUrl(f.path)))) {
        all = false;
        break;
      }
    }
    if (all) out.push(spec.id);
  }
  return out;
}

/** Delete one model, or every model, from this device. */
export async function forgetModel(id?: string): Promise<void> {
  const cache = await openCache();
  if (!cache) return;
  for (const spec of id ? [MODELS[id]].filter(Boolean) : Object.values(MODELS)) {
    for (const f of spec.files) await cache.delete(modelUrl(f.path)).catch(() => false);
  }
}

/** Which models each AI feature needs (for "download size" hints). */
export const FEATURE_MODELS: Record<string, string[]> = {
  selectSubject: ["yunet", "modnet", "u2netp"],
  selectBackground: ["yunet", "modnet", "u2netp"],
  removeBackground: ["yunet", "modnet", "u2netp"],
  blurBackground: ["yunet", "modnet", "u2netp"],
  selectObjectAt: ["edgetam-encoder", "edgetam-decoder"],
  removeFast: ["migan"],
  removeBest: ["lama"],
  upscaleFast: ["realesr-x4v3"],
  upscaleBest: ["realesrgan-x4plus"],
  denoise: ["realesr-x4v3", "realesr-x4v3-dn50", "realesr-x4v3-wdn"],
  removeJpegArtifacts: ["realesr-x4v3"],
  restoreFaces: ["yunet", "gfpgan"],
  colorize: ["deoldify"],
  detectFaces: ["yunet"],
  analyzeScene: ["yunet", "u2netp"],
};

/** Feature names the native server's manifest uses, per model. */
const SERVER_FEATURES: Record<string, string[]> = {
  migan: ["remove-object"],
  lama: ["remove-object"],
  modnet: ["select-subject", "remove-background", "blur-background"],
  u2netp: ["select-subject", "remove-background", "blur-background", "analyze-scene"],
  "edgetam-encoder": ["select-object", "select-subject"],
  "edgetam-decoder": ["select-object", "select-subject"],
  yunet: ["detect-faces", "restore-faces", "analyze-scene", "red-eye"],
  "realesr-x4v3": ["upscale", "jpeg-artifacts", "denoise"],
  "realesr-x4v3-dn50": ["denoise"],
  "realesr-x4v3-wdn": ["denoise"],
  "realesrgan-x4plus": ["upscale"],
  gfpgan: ["restore-faces"],
  deoldify: ["colorize"],
};

/**
 * The native server's `models.json` (crates/editor-server/src/models.rs),
 * one entry per file. `apps/web/public/models.json` is generated from this:
 *   cd apps/web && npx esbuild src/lib/models.ts --format=esm --outfile=/tmp/m.mjs && node -e 'import("/tmp/m.mjs").then(m => console.log(JSON.stringify(m.serverManifest(), null, 2)))' > public/models.json
 */
export function serverManifest() {
  const models = Object.values(MODELS).flatMap((m) =>
    m.files.map((f, i) => ({
      id: i === 0 ? m.id : `${m.id}.data${m.files.length > 2 ? i : ""}`,
      file: f.path,
      url: f.url,
      sha256: f.sha256,
      size: f.bytes,
      license: m.licence,
      attribution: m.attribution,
      features: SERVER_FEATURES[m.id] ?? [],
      tier: m.tier,
      ...(i > 0 ? { part_of: m.id } : {}),
      ...(f.exported ? { exported: true, note: "url is the PyTorch source; the served ONNX is exported by openpixels/scripts/export-models.py and pinned by sha256" } : {}),
      ...(m.greyZone ? { grey_zone: m.greyZone } : {}),
    })),
  );
  return { version: 1, models };
}
