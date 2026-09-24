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
  他の OS 向け配布物に入っているものと同じ振る舞いをする。

.PARAMETER Assemblies
  OpenUtau の配布物を展開したディレクトリ。**アセンブリを全部置く。**
  読むのは `OpenUtau.Core.dll` と `OpenUtau.Plugin.Builtin.dll` の2つだが、
  そこから先の依存はこちらでは数え上げられない（.NOTES）。

.PARAMETER Voicebank
  突合に使う音源1つ（`crates/koeru-core/fixtures/phonemizer-parity/<方式>`）。
  presamp.ini / oto.ini / character.txt / expected.tsv が入っている。
  どれも `crates/koeru-core/tests/phonemizer_parity.rs` が書き出したもの。

.PARAMETER Out
  出力先。KOERU 側の `expected.tsv` と同じ書式（直前・歌詞・綴り）。

.NOTES
  KOERU 側の半分は `crates/koeru-core/tests/phonemizer_parity.rs`。
  あちらが音源と期待値を書き出し、こちらが OpenUtau の答えを出し、
  CI が突き合わせる。

  **ここに置くのは、走らせるのが CI だけだから。** `cargo test` は触らないので、
  `tests/` に置くとテスト対象に見える。`update-hashes.sh` と同じ扱い。

  **音源1つにつき方式1つ。** presamp phonemizer は1つで CV / VCV / CVVC を
  賄い（それが presamp の仕様）、**どの綴りを出すかは音源の中身が決める**
  ——「その綴りの oto があるか」で候補を落としていく。音符1つに返る綴りも
  1つなので、**1つの音源から3方式ぶんの答えは出ない。** CI は方式の数だけ
  このスクリプトを呼ぶ。

  **アセンブリは2つでは足りない。** `OpenUtau.Core.dll` だけを置いて
  `SearchAll` を呼ぶと、`OpenUtau.Core.Util.Preferences` の静的初期化が
  `Serilog` を読めずに落ちる。**踏んだ。** どこまで依存が伸びるかは上流の
  都合で動くので、数え上げずに解決器を置く——配布物のアセンブリを全部
  同じディレクトリへ展開し、要求された名前をそこから返す。
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string] $Assemblies,
  [Parameter(Mandatory)] [string] $Voicebank,
  [Parameter(Mandatory)] [string] $Out
)

$ErrorActionPreference = 'Stop'

$asmDir = (Resolve-Path $Assemblies).Path

# 隣のアセンブリから解決する。 `LoadFrom` は読み込んだものの依存までは
# 面倒を見ないので、置いてあっても名前で探しに行った時点で落ちる。
[System.AppDomain]::CurrentDomain.add_AssemblyResolve({
  param($sender, $e)
  $name = (New-Object Reflection.AssemblyName $e.Name).Name
  $path = Join-Path $asmDir "$name.dll"
  if (Test-Path $path) { return [Reflection.Assembly]::LoadFrom($path) }
  return $null
})

$coreAsm = [Reflection.Assembly]::LoadFrom((Join-Path $asmDir 'OpenUtau.Core.dll'))
$pluginAsm = [Reflection.Assembly]::LoadFrom((Join-Path $asmDir 'OpenUtau.Plugin.Builtin.dll'))

# 名前空間を書かずに、載っているアセンブリから型を引く。
#
# **`[Classic.VoicebankLoader]` と書いて落ちた。** 実体は
# `OpenUtau.Classic`。公開 CLI が無く（`EVID-SYN-001`）アセンブリを
# 直に呼ぶので、名前空間はあちらの都合でいつでも動く。単純名で引けば、
# 動いたときに落ちるのは「型が消えたとき」だけになる。
function Get-Types($asm) {
  try { return $asm.GetTypes() }
  # 依存が欠けて一部が読めなくても、読めたぶんで続ける。
  catch [Reflection.ReflectionTypeLoadException] {
    return @($_.Exception.Types | Where-Object { $null -ne $_ })
  }
}

function Find-Type($asm, [string] $name) {
  $all = Get-Types $asm
  $hit = @($all | Where-Object { $_.Name -eq $name })
  if ($hit.Count -eq 0) {
    $near = @($all | Where-Object { $_.Name -like "*$name*" } | ForEach-Object { $_.FullName })
    throw "型が見つからない: $name`n近いもの: $($near -join ', ')"
  }
  Write-Host "$name -> $($hit[0].FullName)"
  return $hit[0]
}

$loaderType = Find-Type $coreAsm 'VoicebankLoader'
$singerType = Find-Type $coreAsm 'ClassicSinger'
$phonemizerBase = Find-Type $coreAsm 'Phonemizer'
$presampType = Find-Type $pluginAsm 'JapanesePresampPhonemizer'

$noteType = $phonemizerBase.GetNestedType('Note')
$attrType = $phonemizerBase.GetNestedType('PhonemeAttributes')
if ($null -eq $noteType) {
  throw "Phonemizer の入れ子型が見つからない: $(($phonemizerBase.GetNestedTypes() | ForEach-Object { $_.Name }) -join ', ')"
}

# 音源を読む。 `SearchAll` は basePath の下を掘るので、親を渡す。
$bankPath = (Resolve-Path $Voicebank).Path
$base = Split-Path -Parent $bankPath
# 型変数から直に組む。 **`[Activator]::CreateInstance($t, @($x))` は
# 「Constructor not found」で落ちた**——`new VoicebankLoader(String)` は
# 反射で見えているので、束ねた配列の解決に失敗している。
# `$type::new(...)` は通常のメソッド解決を通るので、素直に当たる。
$loader = $loaderType::new([string] $base)
# 掘った先から、名指しされた音源だけを採る。 親の下には方式の数だけ
# 音源が並ぶので、先頭を採ると**別の方式の音源で突き合わせる。**
#
# 見るのは `File`。 **`BasePath` は掘りはじめた親**で、どの音源でも同じ値が
# 入っている（**先頭を採るのと変わらない**）。音源を指しているのは
# `character.txt` の在り処のほう。
#
# 絞り込みにパイプラインを通さない。 `Where-Object` は要素を `PSObject` で
# 包んで返すので、そのまま `LoadVoicebank` へ渡すと
# **「`PSObject` を `Voicebank` へ変換できない」で落ちる。踏んだ。**
$found = @($loader.SearchAll())
$bank = $null
foreach ($b in $found) {
  if ((Split-Path -Parent $b.File) -eq $bankPath) { $bank = $b; break }
}
if ($null -eq $bank) {
  throw "音源が見つからない: $bankPath`n掘れたもの: $(($found | ForEach-Object { $_.File }) -join ', ')"
}
$loaderType.GetMethod('LoadVoicebank').Invoke($null, @($bank))

$singer = $singerType::new($bank)
$singer.EnsureLoaded()
Write-Host "音源: $bankPath（oto $($singer.Otos.Count) 件）"

$phonemizer = $presampType::new()
$phonemizer.SetSinger($singer)

# 呼び出し先の版が上がったとき、実際に読んだ型と `Process` の形を
# CI の失敗ログに残す。公開 CLI ではなくアセンブリを直に呼ぶため、
# これが無いと API の変更と PowerShell の束縛失敗を見分けられない。
Write-Host "note  : $($noteType.FullName)"
Write-Host "fields: $(($noteType.GetFields() | ForEach-Object { $_.Name }) -join ', ')"
$processes = @($phonemizer.GetType().GetMethods() | Where-Object { $_.Name -eq 'Process' })
foreach ($m in $processes) {
  Write-Host "Process($(($m.GetParameters() | ForEach-Object { "$($_.ParameterType.Name) $($_.Name)" }) -join ', '))"
}
# 反射で呼ぶ。 **`$phonemizer.Process(...)` は束縛で落ちた**——戻り値の
# `Phonemizer+Result` は値型で、PowerShell の動的束縛器が
# 「`System.Object` と両立しない」と言う。`MethodInfo.Invoke` は箱に入れて返す。
$processMethod = $processes | Select-Object -First 1
if ($null -eq $processMethod) { throw "Process が見つからない" }

# 型つきの配列を作る。 `@(...)` は `object[]` になるので、
# `Note[]` を取る引数に渡すと束縛に失敗しうる。
function New-NoteArray([object[]] $items = @()) {
  # 空配列は束縛で $null に畳まれることがある。 0 件として扱う。
  $n = if ($null -eq $items) { 0 } else { $items.Count }
  $a = [Array]::CreateInstance($noteType, $n)
  for ($i = 0; $i -lt $n; $i++) { $a[$i] = $items[$i] }
  return , $a
}

function New-Note([string] $lyric, [int] $position) {
  $n = [Activator]::CreateInstance($noteType)
  $n.lyric = $lyric
  $n.tone = 60          # C4。綴りは音高で変わらない（音高の写像は OpenUtau が後で行う）。
  $n.position = $position
  $n.duration = 480     # 4分音符。
  $n.phonemeAttributes = [Array]::CreateInstance($attrType, 0)
  return $n
}

# KOERU 側の期待値をそのまま入力に使う。 同じ音符に当てないと突合にならない。
$rows = Get-Content (Join-Path $bankPath 'expected.tsv') | Where-Object { $_ -notmatch '^#' -and $_.Trim() -ne '' }

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# 直前`t歌詞`t綴り")

foreach ($row in $rows) {
  $cols = $row -split "`t"
  $prevLyric = $cols[0]; $lyric = $cols[1]

  $note = New-Note $lyric 480
  # `Note?` へは値か $null をそのまま渡す。 束縛器が包む——
  # **`[System.Nullable[$noteType]]` と書くと構文解析で落ちる**
  # （型リテラルの中に変数は置けない）。**踏んだ。**
  if ($prevLyric -eq '-') {
    $prev = $null
    $prevs = New-NoteArray @()
  } else {
    $p = New-Note $prevLyric 0
    $prev = $p
    $prevs = New-NoteArray @($p)
  }

  $result = $processMethod.Invoke(
    $phonemizer,
    @((New-NoteArray @($note)), $prev, $null, $prev, $null, $prevs)
  )
  # 音符1つに複数の音素が返ることがある（CVVC の VC）。**先頭を見る。**
  # KOERU の候補列も「その音符を鳴らす綴り」が先頭（`presamp::Rules::candidates`）。
  $alias = if ($result.phonemes.Length -gt 0) { $result.phonemes[0].phoneme } else { '' }
  $lines.Add("$prevLyric`t$lyric`t$alias")
}

# 末尾にも改行を置く。 KOERU 側は1行ずつ改行で閉じて書くので、
# 無いと**中身が同じでも `diff` が最終行を食い違いとして出す。**
Set-Content -Path $Out -Value (($lines -join "`n") + "`n") -NoNewline -Encoding utf8NoBOM
Write-Host "書き出した: $Out ($($lines.Count - 1) 行)"
