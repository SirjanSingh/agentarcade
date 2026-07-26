/**
 * Game selection: uniformly random, never the same game twice in a row.
 * Mirrors src-tauri/src/logic/picker.rs — keep the two in sync.
 */

/** Pick a game, excluding `last` unless that would leave nothing to pick. */
export function chooseExcluding(games: readonly string[], last: string | null, roll: number): string | null {
  if (games.length === 0) return null;

  const candidates = games.filter((g) => g !== last);
  const pool = candidates.length > 0 ? candidates : [...games];
  return pool[roll % pool.length] ?? null;
}
