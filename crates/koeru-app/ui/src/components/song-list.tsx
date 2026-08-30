import { useEffect, useState } from "react";

import { Button } from "~/components/ui/button";
import { Card, CardTitle } from "~/components/ui/card";
import { type SongView, type SungSongView, api, errorMessage } from "~/lib/ipc";

type SongListProps = {
  /** 収録が進むたびに変わる値。**これが変わったら数え直す**（TR-RCL-17）。 */
  revision: number;
};

/**
 * 歌える曲（TR-RCL-17、TR-RCL-19、TR-SYN-20）。
 *
 * **不足は「エイリアス名の一覧」ではなく「あと N 項目で『曲名』が歌える」で出す**
 * （TR-SYN-20）。品質スコア、良し悪しの判定、他音源との比較、上達度は出さない。
 *
 * **曲は録り始めのとっかかりとして最も効く指標であって、唯一の指標ではない**
 * （TR-RCL-19）。曲が1本も無くても、カバレッジで進捗は読める。
 */
export const SongList = ({ revision }: SongListProps) => {
  const [songs, setSongs] = useState<SongView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [sung, setSung] = useState<SungSongView | null>(null);

  const sing = (index: number) => {
    setError(null);
    api
      .singSong(index)
      .then(setSung)
      .catch((e: unknown) => setError(errorMessage(e)));
  };

  useEffect(() => {
    api
      .songStatus()
      .then(setSongs)
      .catch((e: unknown) => setError(errorMessage(e)));
  }, [revision]);

  if (error !== null) {
    return (
      <Card>
        <CardTitle>歌える曲</CardTitle>
        <p role="alert" className="mt-3 text-sm text-danger-text">
          {error}
        </p>
      </Card>
    );
  }

  return (
    <Card>
      <CardTitle>歌える曲</CardTitle>

      {songs.length === 0 ? (
        <p className="mt-3 text-sm text-text-dim">
          曲が入っていません。UST を読み込むと、あと何項目で歌えるかが出ます。
        </p>
      ) : (
        <ul className="mt-3 flex flex-col gap-2">
          {songs.map((s, i) => (
            <li
              key={s.title}
              className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 rounded-lg bg-surface-2 px-4 py-3"
            >
              <span className="font-medium">{s.title}</span>

              {s.missing_units === 0 ? (
                <span className="text-sm text-ok">いま歌えます</span>
              ) : s.singable ? (
                <span className="text-sm text-text-dim">
                  いま歌えます（あと {s.missing_units} 項目で本来の音に）
                </span>
              ) : (
                <span className="font-mono text-sm text-text-dim tabular-nums">
                  あと {s.missing_units} 項目
                </span>
              )}

              {/*
               **代替ありは、音のつながりが粗くなることを1行で説明する**（TR-RCL-19）。
               */}
              {s.singability === "WithFallback" && (
                <p className="w-full text-xs text-text-dim">
                  録っていない単位を近いもので代えるので、音のつながりが粗くなります。
                </p>
              )}

              {/*
                **途中まででも歌わせる**（TR-SYN-18）。鳴らせないフレーズは落とす。
                落とした位置に無音・別音・代替音を挿さない。
              */}
              <div className="flex w-full items-center gap-3">
                <Button onClick={() => sing(i)}>歌わせる</Button>
                <Button variant="ghost" onClick={() => api.stopPreview().catch(() => undefined)}>
                  止める
                </Button>
                {sung?.title === s.title && (
                  <span className="font-mono text-xs text-text-dim tabular-nums">
                    {(sung.duration_ms / 1000).toFixed(1)} 秒
                    {sung.dropped_phrases > 0 &&
                      ` · ${sung.dropped_phrases} フレーズは飛ばしました`}
                  </span>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
};
