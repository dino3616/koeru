# 生成文書

**このディレクトリのファイルは手で編集しない。** `specs/` の FSL 仕様から決定論的に生成している。

内容を変えたいときは FSL 側を直して再生成する。**Markdown から FSL への逆同期は行わない。**

```bash
fslc document generate specs/requirements/project-lifecycle.fsl -o docs/generated/project-lifecycle.md
```

CI は生成物と仕様のずれを検出する。手で書き換えると落ちる。

```bash
fslc document check specs/requirements/project-lifecycle.fsl docs/generated/project-lifecycle.md
```

`background` スロット（`<!-- fsl:slot begin name="background" -->` の内側）だけは自由に編集してよい。ここに規範的な効力はない。
