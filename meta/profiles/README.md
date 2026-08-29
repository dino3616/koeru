# リリースプロファイル

**まだ1つも無い。** マイルストーンを立てるときに、この形で作る。

```toml
id = "PROFILE-<名前>"
title = "..."
status = "in_progress"
includes_fsl = ["REQ-...", "INV-..."]   # FSL が持つ契約
includes_requirements = ["TR-..."]      # meta/requirements が持つ要件
excludes = ["..."]                      # この段階では引き受けないもの
decisions = ["DEC-..."]
budgets = ["BUDGET-..."]
```

未決の論点がこのプロファイルを塞ぐときは、その論点の `blocks_profiles` にプロファイル ID を書く。

```bash
cargo xtask check-profile PROFILE-<名前>
```

**通常の CI では走らせない。** 未決が残っているのは開発中は正常で、その状態でリリースするのが異常だという線引きにしている。実行は `.github/workflows/release-gate.yml`（タグ push と手動）。
