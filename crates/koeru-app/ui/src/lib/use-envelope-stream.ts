import { useEffect, useEffectEvent } from "react";

import { Channel, type EnvelopeView, api } from "~/lib/ipc";

/**
 * いま入ってきている音の包絡を受け続ける（`TR-REC-43`）。 届くたびに `onFrame` を呼ぶ。
 *
 * 包絡は Channel で送られてくる。 `invoke` で引きに行かない（`DEC-PLT-017`）。
 * 間隔は Rust 側が刻むので、画面の都合や IPC の混み具合で速度が変わらない。
 *
 * 1つの部品が1本を持つ。 外れたら止める。 `onFrame` は描画のたびに替わってよく、
 * 替わっても流れは開き直さない。
 */
export const useEnvelopeStream = (onFrame: (frame: EnvelopeView) => void) => {
  const deliver = useEffectEvent(onFrame);

  useEffect(() => {
    let alive = true;
    const channel = new Channel<EnvelopeView>();
    channel.onmessage = (frame) => {
      if (alive) deliver(frame);
    };

    // 開く前は空で正しい。 騒がない。
    // 番号で名指しして止める。 作り直しのときに、
    // 「古いのを止める」より「新しいのを始める」が先に着くことがある。
    let generation: number | null = null;
    api
      .streamEnvelope(channel)
      .then((g) => {
        generation = g;
        // 番号が返る前に外されていたら、ここで止める。
        if (!alive) api.stopEnvelopeStream(g).catch(() => {});
      })
      .catch(() => {});

    return () => {
      alive = false;
      if (generation !== null) api.stopEnvelopeStream(generation).catch(() => {});
    };
  }, []);
};
