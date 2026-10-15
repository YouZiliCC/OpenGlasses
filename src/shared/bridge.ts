// Typed wrappers around Tauri IPC. Always listen on the *current window*:
// the global `listen` from @tauri-apps/api/event also receives events that Rust
// sent with `emit_to` to other windows (DESIGN §3.8 rule 1).
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { CommandMap, EventMap } from "./ipc";

export function call<K extends keyof CommandMap>(
  cmd: K,
  ...args: CommandMap[K][0] extends Record<string, never> ? [] : [CommandMap[K][0]]
): Promise<CommandMap[K][1]> {
  return invoke<CommandMap[K][1]>(cmd, args[0] ?? {});
}

/** Register a listener on this window. Call once per event from main.tsx, before rendering. */
export function on<K extends keyof EventMap>(event: K, fn: (payload: EventMap[K]) => void): Promise<() => void> {
  return getCurrentWebviewWindow().listen<EventMap[K]>(event, (e) => fn(e.payload));
}

export function windowLabel(): string {
  return getCurrentWebviewWindow().label;
}
