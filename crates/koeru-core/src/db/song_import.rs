//! 取り込んだ曲の原文と、取り込みの受領証（`DEC-PLT-054`）。
//!
//! 曲の行は読み取った結果で、持ち込んだファイルそのものではない。 原文を残さないと、
//! パーサを直したときに取り込み直せない。 **曲の行と原文と受領証は、同じトランザクションで
//! 書く。** 別々に書くと、曲だけが入って原文が無い（または受領証だけが入って曲が無い）状態が
//! 落ちたときに残る。
//!
//! 同じ [`OperationId`] の送り直しには、書かずに先の受領証を返す。 応答が失われた
//! consumer が送り直しても、曲は増えない（`DEC-PLT-035`）。 [`OperationId`] が違えば、
//! 原文が同じでも別の取り込みになる——持ち込み直した曲は、一覧にもう1つ並ぶ。

use diesel::prelude::*;
use sha2::{Digest as _, Sha256};

use super::{Ledger, OperationId, Result, db, insert_song};
use crate::schema::{song_import_songs, song_imports};
use crate::song::Song;

/// 取り込みを確定する要求。
#[derive(Debug, Clone, Copy)]
pub struct SongImportRequest<'a> {
    pub operation: &'a OperationId,
    /// ファイルの名前（ディレクトリは含めない）。 取り込み直すとき、形式と題の候補を決めるのに要る。
    pub file_name: &'a str,
    /// 渡されたバイト列そのまま。
    pub bytes: &'a [u8],
    /// 入れる曲。 題は本人が決めたもの（`TR-RCL-12`）で、ID は呼び出し側が新しく振る。
    pub songs: &'a [(String, Song)],
    pub imported_at: &'a str,
}

/// 取り込みを確定した受領証。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongImportReceipt {
    pub operation: OperationId,
    /// 曲の ID。 ファイルの中の並び順。
    pub song_ids: Vec<String>,
    pub imported_at: String,
}

/// 確定せずに返した答え。 どれも失敗ではない（`DEC-PLT-038`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongImportAnswer {
    /// 同じ操作の送り直し。 先に確定したときの受領証。
    Replayed(SongImportReceipt),
    /// 同じ操作の識別子で、別の要求が来た。 先に処理した操作の受領証。 曲は増えていない。
    OperationIdReused(SongImportReceipt),
}

/// [`Ledger::commit_song_import`] の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongImport {
    /// この要求で曲を入れた。
    Imported(SongImportReceipt),
    /// 入れずに答えた。 曲は増えていない。
    Answered(SongImportAnswer),
}

/// 台帳に残っている、曲の原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongSource {
    /// この曲を作った取り込み。
    pub operation: OperationId,
    /// ファイルの中の何番目の曲か（0 始まり。USTX は1トラックが1曲）。
    pub ordinal: usize,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

/// 要求の指紋。 欄の境目をずらした別の要求と同じ値にならないよう、長さを前に置く。
fn request_digest(r: &SongImportRequest<'_>) -> String {
    let mut hasher = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(bytes);
    };
    field(r.file_name.as_bytes());
    field(r.bytes);
    for (_, song) in r.songs {
        field(song.title.as_bytes());
    }
    hasher.finalize().iter().fold(String::new(), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// その操作の取り込みを1件読む。 無ければ `None`。 `(要求の指紋, 受領証)`。
fn load(
    c: &mut diesel::sqlite::SqliteConnection,
    op: &OperationId,
) -> QueryResult<Option<(String, SongImportReceipt)>> {
    let Some((digest, imported_at)) = song_imports::table
        .find(op.as_str())
        .select((song_imports::request_digest, song_imports::imported_at))
        .first::<(String, String)>(c)
        .optional()?
    else {
        return Ok(None);
    };
    let song_ids = song_import_songs::table
        .filter(song_import_songs::operation_id.eq(op.as_str()))
        .order(song_import_songs::ordinal.asc())
        .select(song_import_songs::song_id)
        .load::<String>(c)?;
    Ok(Some((
        digest,
        SongImportReceipt {
            operation: op.clone(),
            song_ids,
            imported_at,
        },
    )))
}

impl Ledger {
    /// 曲と原文と受領証を、同じトランザクションで入れる（`DEC-PLT-054`）。
    ///
    /// 受領証が既にあれば何も書かずに答える。 同じ要求の送り直しは
    /// [`SongImportAnswer::Replayed`]、同じ識別子で別の要求なら
    /// [`SongImportAnswer::OperationIdReused`]。 全部入るか、1つも入らないか。
    ///
    /// 曲の ID は呼び出し側が新しく振る。 既にある曲の ID を渡すと、その曲を差し替えてしまう。
    ///
    /// # Errors
    ///
    /// SQLite の操作が失敗した。
    #[tracing::instrument(skip_all, fields(count = r.songs.len(), len = r.bytes.len()))]
    pub fn commit_song_import(&mut self, r: &SongImportRequest<'_>) -> Result<SongImport> {
        let digest = request_digest(r);
        self.conn
            .transaction(|c| {
                if let Some((stored, receipt)) = load(c, r.operation)? {
                    return Ok(SongImport::Answered(if stored == digest {
                        SongImportAnswer::Replayed(receipt)
                    } else {
                        SongImportAnswer::OperationIdReused(receipt)
                    }));
                }
                diesel::insert_into(song_imports::table)
                    .values((
                        song_imports::operation_id.eq(r.operation.as_str()),
                        song_imports::file_name.eq(r.file_name),
                        song_imports::source_bytes.eq(r.bytes),
                        song_imports::request_digest.eq(&digest),
                        song_imports::imported_at.eq(r.imported_at),
                    ))
                    .execute(c)?;
                let mut song_ids = Vec::with_capacity(r.songs.len());
                for (i, (id, song)) in r.songs.iter().enumerate() {
                    insert_song(c, id, song, false, r.imported_at)?;
                    diesel::insert_into(song_import_songs::table)
                        .values((
                            song_import_songs::operation_id.eq(r.operation.as_str()),
                            song_import_songs::ordinal.eq(i32::try_from(i).unwrap_or(i32::MAX)),
                            song_import_songs::song_id.eq(id),
                        ))
                        .execute(c)?;
                    song_ids.push(id.clone());
                }
                Ok(SongImport::Imported(SongImportReceipt {
                    operation: r.operation.clone(),
                    song_ids,
                    imported_at: r.imported_at.to_owned(),
                }))
            })
            .map_err(db("commit_song_import"))
    }

    /// 取り込みの受領証。 無ければ `None`。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn song_import_receipt(
        &mut self,
        operation: &OperationId,
    ) -> Result<Option<SongImportReceipt>> {
        load(&mut self.conn, operation)
            .map(|r| r.map(|(_, receipt)| receipt))
            .map_err(db("song_import_receipt"))
    }

    /// その曲の原文。 取り込み（この移行より後のもの）が作った曲でなければ `None`。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn song_source(&mut self, song_id: &str) -> Result<Option<SongSource>> {
        let row = song_import_songs::table
            .inner_join(
                song_imports::table
                    .on(song_imports::operation_id.eq(song_import_songs::operation_id)),
            )
            .filter(song_import_songs::song_id.eq(song_id))
            .select((
                song_imports::operation_id,
                song_import_songs::ordinal,
                song_imports::file_name,
                song_imports::source_bytes,
            ))
            .first::<(String, i32, String, Vec<u8>)>(&mut self.conn)
            .optional()
            .map_err(db("song_source"))?;
        Ok(
            row.map(|(operation, ordinal, file_name, bytes)| SongSource {
                // 書くときに揃えてあるので、読み直さない。
                operation: OperationId::stored(operation),
                ordinal: usize::try_from(ordinal).unwrap_or_default(),
                file_name,
                bytes,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::{Note, Provenance};

    fn song(title: &str) -> Song {
        Song {
            title: title.to_owned(),
            notes: vec![Note {
                lyric: "さ".to_owned(),
                midi: 60,
                ticks: 480,
                rest_ticks: 0,
            }],
            provenance: Provenance {
                source: "本人が持ち込んだ UST".to_owned(),
                license: "不明（配布物には含めない）".to_owned(),
            },
            tempo_bpm: 120.0,
            default_portamento_ms: 0.0,
            transpose: 0,
        }
    }

    fn request<'a>(
        op: &'a OperationId,
        bytes: &'a [u8],
        songs: &'a [(String, Song)],
    ) -> SongImportRequest<'a> {
        SongImportRequest {
            operation: op,
            file_name: "ふるさと.ust",
            bytes,
            songs,
            imported_at: "2026-10-13T00:00:00Z",
        }
    }

    fn ids(l: &mut Ledger) -> Vec<String> {
        let mut v: Vec<String> = l
            .all_songs()
            .expect("引ける")
            .into_iter()
            .map(|(id, ..)| id)
            .collect();
        v.sort();
        v
    }

    /// 原文は渡されたバイト列のまま残る。 符号化も改行も直さない。
    #[test]
    fn 原文をそのまま残す() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let op = OperationId::generate();
        // CP932 の「さ」と CRLF。 UTF-8 へも LF へも直さない。
        let bytes: &[u8] = b"[#0000]\r\nLyric=\x82\xb3\r\n";
        let songs = vec![("a".to_owned(), song("A"))];
        let SongImport::Imported(receipt) = l
            .commit_song_import(&request(&op, bytes, &songs))
            .expect("入る")
        else {
            panic!("入ること");
        };
        assert_eq!(receipt.song_ids, ["a"]);

        let source = l.song_source("a").expect("引ける").expect("原文がある");
        assert_eq!(source.bytes, bytes);
        assert_eq!(source.file_name, "ふるさと.ust");
        assert_eq!(source.operation, op);
        assert_eq!(source.ordinal, 0);
        // 取り込みを経ていない曲（同梱曲など）は原文を持たない。
        assert_eq!(l.song_source("bundled-0").expect("引ける"), None);
    }

    /// 同じ要求の送り直しは何も書かず、先の ID を返す。
    #[test]
    fn 同じ操作の送り直しは書かずに先の_id_を返す() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let op = OperationId::generate();
        let first = vec![("a".to_owned(), song("A")), ("b".to_owned(), song("B"))];
        let SongImport::Imported(receipt) = l
            .commit_song_import(&request(&op, b"x", &first))
            .expect("入る")
        else {
            panic!("入ること");
        };
        let revision = l.revision().expect("読める");

        // 送り直しは新しい ID を振ってくる。 台帳の ID が勝つ。
        let again = vec![("c".to_owned(), song("A")), ("d".to_owned(), song("B"))];
        let answer = l
            .commit_song_import(&request(&op, b"x", &again))
            .expect("答える");
        assert_eq!(
            answer,
            SongImport::Answered(SongImportAnswer::Replayed(receipt))
        );
        assert_eq!(ids(&mut l), ["a", "b"], "曲は増えない");
        assert_eq!(l.revision().expect("読める"), revision, "何も書かない");
    }

    /// 同じ識別子で別の要求は、送り直しではない。 先の受領証を返し、何も書かない。
    #[test]
    fn 同じ識別子で別の要求は先の受領証を返す() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let op = OperationId::generate();
        let first = vec![("a".to_owned(), song("A"))];
        l.commit_song_import(&request(&op, b"x", &first))
            .expect("入る");

        // バイト列が違う。
        let other = vec![("b".to_owned(), song("A"))];
        let SongImport::Answered(SongImportAnswer::OperationIdReused(r)) = l
            .commit_song_import(&request(&op, b"y", &other))
            .expect("答える")
        else {
            panic!("別の要求として答えること");
        };
        assert_eq!(r.song_ids, ["a"]);
        // 題が違う。
        let retitled = vec![("c".to_owned(), song("別の題"))];
        assert!(matches!(
            l.commit_song_import(&request(&op, b"x", &retitled))
                .expect("答える"),
            SongImport::Answered(SongImportAnswer::OperationIdReused(_))
        ));
        assert_eq!(ids(&mut l), ["a"], "曲は増えない");
    }

    /// 識別子が違えば、原文が同じでも別の取り込み。 持ち込み直した曲はもう1組並ぶ。
    #[test]
    fn 別の識別子なら同じ原文でも別の取り込み() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let first = vec![("a".to_owned(), song("A"))];
        let second = vec![("b".to_owned(), song("A"))];
        l.commit_song_import(&request(&OperationId::generate(), b"x", &first))
            .expect("入る");
        let op2 = OperationId::generate();
        assert!(matches!(
            l.commit_song_import(&request(&op2, b"x", &second))
                .expect("入る"),
            SongImport::Imported(_)
        ));
        assert_eq!(ids(&mut l), ["a", "b"]);
        assert_eq!(
            l.song_source("b").expect("引ける").expect("ある").operation,
            op2
        );
    }

    /// 途中で落ちたら、曲も原文も受領証も残らない。 やり直すと同じ曲が重なる、を防ぐ。
    #[test]
    fn 途中で落ちたら何も残らない() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let op = OperationId::generate();
        // 同じ ID を2曲に振る。 2曲目の対応表への書き込みが一意制約で落ちる。
        let clash = vec![("a".to_owned(), song("A")), ("a".to_owned(), song("B"))];
        assert!(l.commit_song_import(&request(&op, b"x", &clash)).is_err());

        assert!(ids(&mut l).is_empty(), "曲が残らない");
        assert_eq!(l.song_import_receipt(&op).expect("引ける"), None);
        assert_eq!(l.song_source("a").expect("引ける"), None);

        // 同じ識別子でやり直せる。
        let fine = vec![("a".to_owned(), song("A"))];
        assert!(matches!(
            l.commit_song_import(&request(&op, b"x", &fine))
                .expect("入る"),
            SongImport::Imported(_)
        ));
    }
}
