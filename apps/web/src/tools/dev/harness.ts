// Development harness for the canvas tools: the real canvas, a tool picker
// and the options bar, without either shell. Served by Vite at
// /src/tools/dev/harness.html; driven by apps/web/e2e/tools.spec.mjs.
import { mount } from "svelte";
import "../../styles/app.css";
import ToolHarness from "./ToolHarness.svelte";

const params = new URLSearchParams(location.search);
if (params.get("theme") === "light") {
  document.documentElement.classList.remove("oa-dark");
  document.documentElement.classList.add("oa-light");
}

mount(ToolHarness, { target: document.getElementById("app")! });
