// Long-running jobs the worker can run. Each module registers its handlers
// here by name; the UI calls them through `engine.job(name, params)`.
import type { JobHandler } from "./types";

export const JOBS: Record<string, JobHandler> = {};

export function register(handlers: Record<string, JobHandler>) {
  Object.assign(JOBS, handlers);
}
