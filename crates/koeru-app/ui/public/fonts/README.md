# 同梱する書体

`DEC-PLT-026`。Noto Sans JP の可変フォント1本だけを置く。3 OS で顔を揃えるため。

|                      |                                                   |
| -------------------- | ------------------------------------------------- |
| `noto-sans-jp.woff2` | Noto Sans JP（可変、`wght` 100–900）。SIL OFL 1.1 |
| `OFL.txt`            | 上のライセンス本文。**同梱が条件なので消さない**  |

出どころは Google Fonts の [`ofl/notosansjp/NotoSansJP[wght].ttf`](https://github.com/google/fonts/tree/main/ofl/notosansjp)。
WOFF2 へ包み直しただけで、字は1つも落としていない（`DEC-PLT-026` の「サブセット化しない」）。
包み直すと 9.6MB が 4.3MB になる。

**cmap は 16,732、うち非 BMP は 655。** ここで実測した数で、`DEC-PLT-026` も
これに揃えてある（以前は「非 BMP を 2000 字超持つ」と書いていた）。
他の候補（常用域まで）より広いという結論は変わらない。

**このファイルを見ている検査は無い。** `cargo deny check` は Cargo の依存、
`check-licenses.ts` は JS の依存を見ていて、どちらもここへ当たらない。
当面の検査点は部品台帳の `CMP-131` だけ（`DEC-PLT-026` の「検査の穴」）。
