<script lang="ts">
  // Light, Color, Black and white, Detail: one master slider each, a
  // disclosure for the fine controls, and a switch to compare the group on
  // and off. Everything drives the one "Adjustments" develop layer.
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import { editor } from "../../lib/editor.svelte";
  import { t } from "../../lib/i18n";
  import { DEVELOP_DEFAULT, type Develop } from "../../engine/types";
  import Slider from "../components/Slider.svelte";
  import { GROUPS, groupKeys, isGroupNeutral, zeroed, type Group } from "../develop";
  import { adjustments, lite } from "../lite.svelte";

  let open = $state<Record<string, boolean>>({});
  /** Values a switched-off group had, per layer, so switching on restores them. */
  let stash = $state<Record<string, Partial<Develop>>>({});

  const d = $derived(adjustments.values);
  const layerId = $derived(adjustments.layer?.id ?? null);
  const hasChanges = $derived((Object.keys(DEVELOP_DEFAULT) as (keyof Develop)[]).some((k) => k !== "vignette" && k !== "grain" && d[k] !== 0));

  function stashKey(g: Group) {
    return `${layerId}:${g.id}`;
  }

  function isOff(g: Group) {
    const s = stash[stashKey(g)];
    return !!s && isGroupNeutral(g, d);
  }

  function setMaster(g: Group, m: number) {
    clearStash(g);
    adjustments.set(g.fromMaster(m));
  }

  function setSub(g: Group, key: keyof Develop, v: number) {
    clearStash(g);
    adjustments.set({ [key]: v });
  }

  function clearStash(g: Group) {
    const k = stashKey(g);
    if (stash[k]) {
      const next = { ...stash };
      delete next[k];
      stash = next;
    }
  }

  async function toggle(g: Group, on: boolean) {
    const k = stashKey(g);
    if (!on) {
      const saved = Object.fromEntries(groupKeys(g).map((key) => [key, d[key]]));
      if (isGroupNeutral(g, d)) return;
      await adjustments.replace({ ...d, ...zeroed(groupKeys(g)) });
      stash = { ...stash, [`${adjustments.layer?.id ?? null}:${g.id}`]: saved };
      lite.note(t("Turn off {group}", { group: t(g.label) }));
    } else if (stash[k]) {
      const saved = stash[k];
      clearStash(g);
      await adjustments.replace({ ...d, ...saved });
      lite.note(t("Turn on {group}", { group: t(g.label) }));
    }
  }

  async function commit(g: Group, sub?: string) {
    await adjustments.commit();
    const m = g.toMaster(adjustments.stored);
    lite.note(sub ?? `${t(g.label)} ${m > 0 && g.min < 0 ? "+" : ""}${m}`, /Develop|Adjustment/);
  }

  async function resetAll() {
    stash = {};
    await adjustments.replace({ ...DEVELOP_DEFAULT, vignette: d.vignette, grain: d.grain });
    lite.note(t("Reset adjustments"));
  }

  function fmtSub(key: keyof Develop) {
    return (v: number) => (key === "exposure" ? `${v > 0 ? "+" : ""}${v.toFixed(2)}` : v > 0 ? `+${Math.round(v)}` : `${Math.round(v)}`);
  }
</script>

<div class="adjust">
  {#each GROUPS as g (g.id)}
    {@const off = isOff(g)}
    {@const master = g.toMaster(d)}
    <section class="group" class:off data-testid="group-{g.id}">
      <div class="top">
        <input
          type="checkbox"
          class="oa-checkbox"
          checked={!off}
          disabled={!editor.hasDocument}
          aria-label={t("Apply {group}", { group: t(g.label) })}
          onchange={(e) => toggle(g, e.currentTarget.checked)}
        />
        <div class="master">
          <Slider
            label={t(g.label)}
            value={master}
            min={g.min}
            max={g.max}
            emphasis
            disabled={!editor.hasDocument}
            testid="slider-{g.id}"
            oninput={(v) => setMaster(g, v)}
            oncommit={() => commit(g)}
          />
        </div>
        <button
          class="oa-icon-btn disclose"
          class:open={open[g.id]}
          aria-expanded={!!open[g.id]}
          aria-label={open[g.id] ? t("Hide {group} options", { group: t(g.label) }) : t("Show {group} options", { group: t(g.label) })}
          onclick={() => (open = { ...open, [g.id]: !open[g.id] })}
        >
          <ChevronDown size={18} />
        </button>
      </div>
      {#if open[g.id]}
        <div class="subs">
          {#each g.subs as s (s.key)}
            <Slider
              label={t(s.label)}
              value={d[s.key]}
              min={s.min}
              max={s.max}
              step={s.step}
              format={fmtSub(s.key)}
              disabled={!editor.hasDocument}
              testid="slider-{s.key}"
              oninput={(v) => setSub(g, s.key, v)}
              oncommit={() => commit(g, t(s.label))}
            />
          {/each}
        </div>
      {/if}
    </section>
  {/each}
  <div class="foot">
    <p class="lt-hint">{t("Double-click a slider to reset it.")}</p>
    <button class="oa-btn oa-btn--ghost" disabled={!hasChanges} onclick={resetAll}>
      <RotateCcw size={14} />
      {t("Reset all")}
    </button>
  </div>
</div>

<style>
  .adjust {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .group {
    border-radius: var(--radius-lg);
    padding: var(--space-3) var(--space-2) var(--space-2) var(--space-3);
    background: var(--bg-subtle);
    border: var(--border-width) solid var(--border-hairline);
    transition: opacity var(--duration-fast) var(--ease-standard);
  }
  .top {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-3);
  }
  .top input[type="checkbox"] {
    margin: 0;
    align-self: start;
    margin-top: 4px;
  }
  .off .master {
    opacity: 0.55;
  }
  .disclose {
    align-self: start;
    transition: transform var(--duration-base) var(--ease-standard), var(--transition-control);
  }
  .disclose.open {
    transform: rotate(180deg);
  }
  .subs {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: var(--space-2) calc(var(--control-h-md) + var(--space-3)) var(--space-1) calc(15px + var(--space-3));
    padding-top: var(--space-3);
    border-top: var(--border-width) solid var(--border-hairline);
    animation: open var(--duration-base) var(--ease-out);
  }
  @keyframes open {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }
</style>
