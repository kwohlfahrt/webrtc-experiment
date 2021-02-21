import { useState, useCallback, useRef } from "react";

export interface Pos {
  x: number;
  y: number;
}

const distance = (a: Pos, b: Pos): number => {
  const x = a.x - b.x;
  const y = a.y - b.y;

  return Math.sqrt(x * x + y * y);
};

export const factor = (a: Pos, b: Pos): number => {
  const dist = distance(a, b);
  return 1 - Math.min(1, Math.max(0, dist - 200) / 400)
}

export const useMap = <K, V>(): [
  Map<K, V>,
  { insert: (k: K, v: V) => void; remove: (k: K) => void },
] => {
  const [state, setState] = useState<Map<K, V>>(new Map());

  const insert = useCallback(
    (k, v) => setState((state) => new Map([...state, [k, v]])),
    [],
  );

  const remove = useCallback((k) => {
    setState((state) => {
      const newState = new Map(state);
      newState.delete(k);
      return newState;
    });
  }, []);

  return [state, { insert, remove }];
};
