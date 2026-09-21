<#
.SYNOPSIS
  OpenUtau の presamp phonemizer に同じ表を当て、同じ綴りが出るかを見る。

.DESCRIPTION
  `DEC-SYN-010` の層B。 KOERU が書き出す presamp.ini を OpenUtau が
  どう読むかは、こちらの実装からは分からない。**同じ音源と同じ音符を
  あちらに食わせて、出てきた綴りを突き合わせるしかない。**

  OpenUtau に公式の CLI は無い（`EVID-SYN-001`）。 配布物の中の .NET
  アセンブリを直接読み込んで、phonemizer を呼ぶ。

  **Windows は要らない。** 綴りを決めるのは managed なコードで、
  Linux 向け配布物に入っているアセンブリと同じ IL。

  読むのは2つだけ。
    OpenUtau.Core.dll            Classic.VoicebankLoader / ClassicSinger / Presamp
    OpenUtau.Plugin.Builtin.dll  JapanesePresampPhonemizer

.PARAMETER Assemblies
  上の2つの DLL が置いてあるディレクトリ。

.PARAMETER Voicebank
  突合に使う音源（`crates/koeru-core/fixtures/phonemizer-parity`）。
  presamp.ini / oto.ini / character.txt / expected.tsv が入っている。
  どれも `crates/koeru-core/tests/phonemizer_parity.rs` が書き出したもの。

.PARAMETER Out
  出力先。KOERU 側の `expected.tsv` と同じ書式（方式・直前・歌詞・綴り）。

.NOTES
  KOERU 側の半分は `crates/koeru-core/tests/phonemizer_parity.rs`。
  あちらが音源と期待値を書き出し、こちらが OpenUtau の答えを出し、
  CI が突き合わせる。

  **ここに置くのは、走らせるのが CI だけだから。** `cargo test` は触らないので、
  `tests/` に置くとテスト対象に見える。`update-hashes.sh` と同じ扱い。

  方式ごとに phonemizer を分けない。 presamp phonemizer は1つで CV / VCV /
  CVVC を賄う（それが presamp の仕様）。**どの綴りを出すかは音源の中身が決める。**
  だからこちらは「直前の音符があるかどうか」だけを変えて呼び、
  出てきた綴りが KOERU の方式ごとの候補の先頭と一致するかを見る。
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string] $Assemblies,
  [Parameter(Mandatory)] [string] $Voicebank,
  [Parameter(Mandatory)] [string] $Out
)

$ErrorActionPreference = 'Stop'

Add-Type -Path (Join-Path $Assemblies 'OpenUtau.Core.dll')
Add-Type -Path (Join-Path $Assemblies 'OpenUtau.Plugin.Builtin.dll')

# 音源を読む。 `SearchAll` は basePath の下を掘るので、親を渡す。
$base = Split-Path -Parent (Resolve-Path $Voicebank)
$loader = [Classic.VoicebankLoader]::new($base)
$bank = @($loader.SearchAll())[0]
if ($null -eq $bank) { throw "音源が見つからない: $Voicebank" }
[Classic.VoicebankLoader]::LoadVoicebank($bank)

$singer = [Classic.ClassicSinger]::new($bank)
$singer.EnsureLoaded()

$phonemizer = [OpenUtau.Plugin.Builtin.JapanesePresampPhonemizer]::new()
$phonemizer.SetSinger($singer)

$noteType = [OpenUtau.Api.Phonemizer+Note]

function New-Note([string] $lyric, [int] $position) {
  $n = [Activator]::CreateInstance($noteType)
  $n.lyric = $lyric
  $n.tone = 60          # C4。綴りは音高で変わらない（音高の写像は OpenUtau が後で行う）。
  $n.position = $position
  $n.duration = 480     # 4分音符。
  $n.phonemeAttributes = @()
  return $n
}

# KOERU 側の期待値をそのまま入力に使う。 同じ音符に当てないと突合にならない。
$rows = Get-Content (Join-Path $Voicebank 'expected.tsv') | Where-Object { $_ -notmatch '^#' -and $_.Trim() -ne '' }

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# 方式`t直前`t歌詞`t綴り")

foreach ($row in $rows) {
  $cols = $row -split "`t"
  $method = $cols[0]; $prevLyric = $cols[1]; $lyric = $cols[2]

  $note = New-Note $lyric 480
  if ($prevLyric -eq '-') {
    $prev = $null
    $prevs = @()
  } else {
    $p = New-Note $prevLyric 0
    $prev = [System.Nullable[$noteType]]::new($p)
    $prevs = @($p)
  }

  $result = $phonemizer.Process(@($note), $prev, $null, $prev, $null, $prevs)
  # 音符1つに複数の音素が返ることがある（CVVC の VC）。**先頭を見る。**
  # KOERU の候補列も「その音符を鳴らす綴り」が先頭（`presamp::Rules::candidates`）。
  $alias = if ($result.phonemes.Length -gt 0) { $result.phonemes[0].phoneme } else { '' }
  $lines.Add("$method`t$prevLyric`t$lyric`t$alias")
}

Set-Content -Path $Out -Value ($lines -join "`n") -NoNewline -Encoding utf8NoBOM
Write-Host "書き出した: $Out ($($lines.Count - 1) 行)"
