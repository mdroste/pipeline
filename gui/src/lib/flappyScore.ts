export const FLAPPY_HIGH_SCORE_KEY = "pipeline.easter-egg.flappy.high-score.v1";

export function readFlappyHighScore(
  storage: Pick<Storage, "getItem"> = localStorage,
): number {
  try {
    const value = Number(storage.getItem(FLAPPY_HIGH_SCORE_KEY));
    return Number.isSafeInteger(value) && value > 0 ? value : 0;
  } catch {
    return 0;
  }
}

export function persistFlappyHighScore(
  candidate: number,
  current: number,
  storage: Pick<Storage, "setItem"> = localStorage,
): number {
  const safeCandidate =
    Number.isSafeInteger(candidate) && candidate > 0 ? candidate : 0;
  const safeCurrent =
    Number.isSafeInteger(current) && current > 0 ? current : 0;
  const best = Math.max(safeCandidate, safeCurrent);
  try {
    storage.setItem(FLAPPY_HIGH_SCORE_KEY, String(best));
  } catch {
    // A disabled or full storage area should not prevent the game from working.
  }
  return best;
}
