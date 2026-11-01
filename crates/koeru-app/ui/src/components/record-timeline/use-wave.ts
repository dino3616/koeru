import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import type { EnvelopeView } from "~/lib/ipc";
import { takeBarsQuery } from "~/lib/queries";
import { useEnvelopeStream } from "~/lib/use-envelope-stream";

import { WAVE_BARS } from ".";

/** 包絡の目盛り1本の長さ（ミリ秒）。 Rust 側の刻み（`EnvelopeView.steps`）。 */
const STEP_MS = 5;

/**
 * 波形の欄が何を出すか。
 *
 * - `live`: いま入っている音。 テイクが無い待機中とカウント中（`DEC-REC-013`）
 * - `recording`: 録っている音。 尺の上に積む
 * - `hold`: 録り終えて確定を待つあいだ。 積んだものをそのまま見せる
 * - `take`: 確定したテイク
 */
export type WaveSource =
  | { kind: "live" }
  | { kind: "recording" }
  | { kind: "hold" }
  | { kind: "take"; takeId: number; durationMs: number };

type Options = {
  voiceId: string;
  source: WaveSource;
  beatMs: number;
  /** 尺の長さ（`timelineCells` × 拍）。 */
  spanMs: number;
};

const amplitude = ([lo, hi]: readonly [number, number]) => Math.max(-lo, hi);

/**
 * 波形の欄の棒（`WAVE_BARS` 本）を作る。 包絡の Channel は `useEnvelopeStream` で1本だけ持ち、
 * 新しい Channel は足さない（`DEC-PLT-036`）。
 *
 * 録っているあいだは、届いた目盛りを尺の時刻へ置いて積む。 時刻は録りはじめからの
 * 画面の時計で数える。 目盛りの窓は録りはじめより前も含むので、助走のマスにも
 * 過ぎたカウントの音が入る。
 */
export const useTimelineWave = ({ voiceId, source, beatMs, spanMs }: Options) => {
  /** 録った音。 尺の時刻を `STEP_MS` で割った位置ごとの大きさ。 */
  const [recorded, setRecorded] = useState<readonly (number | undefined)[]>([]);
  const [live, setLive] = useState<readonly [number, number][]>([]);
  const startedAt = useRef(0);
  /** 録りはじめてから、まだ1つも積んでいない。 前のテイクの音を捨てる合図。 */
  const fresh = useRef(false);

  const recording = source.kind === "recording";
  useEffect(() => {
    if (!recording) return;
    startedAt.current = performance.now();
    fresh.current = true;
  }, [recording]);

  useEnvelopeStream((frame: EnvelopeView) => {
    if (source.kind === "recording") {
      const now = performance.now() - startedAt.current + beatMs;
      const reset = fresh.current;
      fresh.current = false;
      const last = frame.steps.length - 1;
      setRecorded((prev) => {
        const next = reset ? [] : prev.slice();
        frame.steps.forEach((step, k) => {
          const at = Math.floor((now - (last - k) * STEP_MS) / STEP_MS);
          if (at >= 0) next[at] = amplitude(step);
        });
        return next;
      });
    } else if (source.kind === "live") {
      setLive(frame.steps);
    }
  });

  const take = useQuery({
    ...takeBarsQuery(
      voiceId,
      source.kind === "take" ? source.takeId : -1,
      source.kind === "take" ? source.durationMs : 0,
      source.kind === "take" ? Math.ceil((source.durationMs / spanMs) * WAVE_BARS) : 0,
    ),
    enabled: source.kind === "take",
  });

  const fromRecorded = (): (number | null)[] => {
    const steps = recorded;
    return Array.from({ length: WAVE_BARS }, (_, i) => {
      const from = Math.floor((i * spanMs) / WAVE_BARS / STEP_MS);
      const to = Math.max(from + 1, Math.floor(((i + 1) * spanMs) / WAVE_BARS / STEP_MS));
      let top: number | null = null;
      for (let s = from; s < to; s += 1) {
        const v = steps[s];
        if (v !== undefined) top = Math.max(top ?? 0, v);
      }
      return top;
    });
  };

  if (source.kind === "live") {
    if (live.length === 0) return Array.from({ length: WAVE_BARS }, () => null);
    return Array.from({ length: WAVE_BARS }, (_, i) => {
      const step = live[Math.floor((i * live.length) / WAVE_BARS)];
      return step === undefined ? null : amplitude(step);
    });
  }
  if (source.kind === "take") {
    const points = take.data;
    if (points === undefined) return Array.from({ length: WAVE_BARS }, () => null);
    return Array.from({ length: WAVE_BARS }, (_, i) => {
      const ms = ((i + 0.5) * spanMs) / WAVE_BARS - beatMs;
      // 助走のマスはテイクの外。 録った音が無いのではなく、鳴らしていない。
      if (ms < 0) return 0;
      if (ms >= source.durationMs) return null;
      const point = points[Math.floor((ms / source.durationMs) * points.length)];
      return point === undefined ? null : amplitude(point);
    });
  }
  return fromRecorded();
};
