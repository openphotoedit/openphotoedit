# Tool ids

The canvas dispatches pointer input to `TOOLS[id]` (`apps/web/src/tools/registry.ts`). Shells show tools by these ids; the tools workstream implements them. Photoshop's shortcut letter in brackets.

| id | Tool | Profiles |
|---|---|---|
| `move` [V] | Move layer / auto-select, arrow-key nudge | Pro, Lite (move object) |
| `marquee-rect` [M], `marquee-ellipse` | Marquee selections (shift = add/square, alt = subtract/centre) | Pro |
| `lasso` [L], `polygon-lasso` | Lasso selections | Pro, Lite (circle to select) |
| `magic-wand` [W], `quick-select` | Colour / edge-aware selections | Pro |
| `object-select` | Box or click to select an object (AI) | Pro, Lite (tap to select) |
| `crop` [C] | Crop with ratio, exact size, straighten, rule-of-thirds overlay | both |
| `perspective-crop` | Four-corner crop | Pro |
| `eyedropper` [I] | Sample colour (merged or layer) | both |
| `spot-heal` [J], `heal`, `remove`, `patch`, `red-eye` | Retouching | Pro; Lite gets `remove` and `spot-heal` |
| `brush` [B], `pencil`, `eraser` [E] | Painting | Pro; Lite gets `pen`/`highlighter` shapes instead |
| `clone` [S] | Clone stamp (alt-click source) | Pro |
| `gradient` [G], `bucket` | Fills | Pro |
| `dodge` [O], `burn`, `sponge`, `blur-brush`, `sharpen-brush`, `smudge` | Toning brushes | Pro |
| `text` [T] | Text layers (click point text, drag paragraph text) | both |
| `shape-rect` [U], `shape-ellipse`, `shape-line` | Live shape layers | both |
| `arrow`, `pen` (freehand polyline), `highlighter`, `redact`, `pixelate` | Annotation (OpenCapture's tools, with colour and width) | Lite first, also Pro |
| `transform` [Ctrl/Cmd+T] | Free transform handles on the active layer or selection | both |
| `hand` [H], `zoom` [Z] | Navigation (implemented) | both |

Settings live in `apps/web/src/tools/settings.svelte.ts` (`toolSettings`), and each tool with options exports a compact options component from `apps/web/src/tools/options/` that both shells can mount (Pro in its options bar, Lite in its bottom sheet).
