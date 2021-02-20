import { useState, useCallback } from "react";

export interface Pos {
  x: number;
  y: number;
}

export const distance = (a: Pos, b: Pos): number => {
  const x = a.x - b.x;
  const y = a.y - b.y;

  return Math.sqrt(x * x + y * y);
};

export const useMap = <K, V>(): [
  Map<K, V>,
  (k: K, v: V) => void,
  (k: K) => void,
] => {
  const [state, setState] = useState<Map<K, V>>(new Map());
  const insert = useCallback((k, v) => setState(new Map([...state, [k, v]])), [
    state,
  ]);
  const remove = useCallback(
    (k) => {
      const newState = new Map(state);
      newState.delete(k);
      setState(newState);
    },
    [state],
  );

  return [state, insert, remove];
};
