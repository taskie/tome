export function rejectEmpty<K extends string, V extends string | null | undefined, M extends Record<K, V>>(
  m: M,
): Partial<M> {
  const result: Partial<M> = {};
  for (const [k, v] of Object.entries(m) as [K, V][]) {
    if (v != null && v.length !== 0) {
      (result[k] as V) = v;
    }
  }
  return result;
}
