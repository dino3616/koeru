/** ティックと音高を、描く座標へ写す。 */

/** 4分音符のティック数（`NoteView.ticks` の単位。480 が4分音符）。 */
export const TICKS_PER_BEAT = 480;

/** 1小節の横幅。 小節の長さが変わっても、見える密度は変えない。 */
export const BAR_WIDTH = 160;

/** 音高1つの高さ。 */
export const ROW_HEIGHT = 20;

/** 上の小節番号の帯の高さ。 */
export const RULER_HEIGHT = 24;

/** 左の鍵盤の幅。 */
export const KEYS_WIDTH = 56;

export type PitchRange = {
  /** 下端の MIDI 番号（含む）。 */
  low: number;
  /** 上端の MIDI 番号（含む）。 */
  high: number;
};

export type PianoRollGeometry = {
  /** 小節の長さ（ティック）。 */
  ticksPerBar: number;
  /** 小節の数。 音符が1つも無くても 1 以上。 */
  bars: number;
  /** 見せる音高。 */
  pitchRange: PitchRange;
  /** 図の幅。 鍵盤は含まない。 */
  width: number;
  /** 図の高さ。 小節番号の帯を含む。 */
  height: number;
  /** ティックから x へ。 */
  tickToX: (tick: number) => number;
  /** x からティックへ。 */
  xToTick: (x: number) => number;
  /** 音高の行の上端の y。 範囲の外でも式は延びる。 */
  midiToY: (midi: number) => number;
  /** y から、その行の MIDI 番号へ。 小節番号の帯の中では範囲の上端より上を返す。 */
  yToMidi: (y: number) => number;
};

/**
 * 描く座標の写像。
 *
 * #456 の範囲ハンドルのように、図の上に重ねるものが同じ座標で描けるように、
 * 部品が持っている写像をそのまま出す。 重ねる側が式を持ち直すと、
 * 小節の幅や行の高さを変えたときに、ずれるのは重ねたものだけになる。
 */
export const pianoRollGeometry = ({
  ticksPerBar,
  endTick,
  pitchRange,
}: {
  ticksPerBar: number;
  /** いちばん後ろの音符の終わり。 */
  endTick: number;
  pitchRange: PitchRange;
}): PianoRollGeometry => {
  const bars = Math.max(1, Math.ceil(endTick / ticksPerBar));
  const rows = pitchRange.high - pitchRange.low + 1;
  const pxPerTick = BAR_WIDTH / ticksPerBar;
  return {
    ticksPerBar,
    bars,
    pitchRange,
    width: bars * BAR_WIDTH,
    height: RULER_HEIGHT + rows * ROW_HEIGHT,
    tickToX: (tick) => tick * pxPerTick,
    xToTick: (x) => x / pxPerTick,
    midiToY: (midi) => RULER_HEIGHT + (pitchRange.high - midi) * ROW_HEIGHT,
    yToMidi: (y) => pitchRange.high - Math.floor((y - RULER_HEIGHT) / ROW_HEIGHT),
  };
};
